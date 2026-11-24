#!/usr/bin/env python3
import argparse
from contextlib import contextmanager
from ipaddress import ip_address
import json
from pathlib import Path
import re
import shlex
import signal
import socket
import subprocess
import sys
from tempfile import TemporaryDirectory
from uuid import uuid4


SSH = [
    "ssh", "-T", "-x", "-a", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
    "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=10", "-o", "ServerAliveCountMax=3",
]
DIRECT_OPTIONS = (
    "hostname", "user", "port", "hostkeyalias", "checkhostip", "userknownhostsfile",
    "globalknownhostsfile", "revokedhostkeys", "hostkeyalgorithms", "casignaturealgorithms",
    "bindaddress", "bindinterface",
)


def checked(stage, command, **options):
    try:
        return subprocess.run(command, check=True, **options)
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(f"{stage}超时（{error.timeout} 秒）") from error
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"{stage}失败（退出码 {error.returncode}）") from error


def ssh_settings(host):
    result = checked("解析 SSH 目标", [*SSH, "-G", "--", host], capture_output=True, text=True, timeout=10)
    options = dict(line.split(" ", 1) for line in result.stdout.splitlines())
    if any(options.get(key, "none") != "none" for key in ("proxyjump", "proxycommand", "knownhostscommand")):
        raise ValueError("临时密钥模式需要可通过 known_hosts 校验的直连 SSH 目标")
    return options


def verify_control(bootstrap, python, settings):
    identify = ("import json,os,pwd; "
                "print(json.dumps([pwd.getpwuid(os.getuid()).pw_name, os.environ['SSH_CONNECTION']]))")
    result = checked("核对初次授权连接的宿主", [*bootstrap, shlex.join([python, "-I", "-B", "-c", identify])],
                     capture_output=True, text=True, timeout=8)
    user, connection = json.loads(result.stdout)
    _, _, address, port = connection.split()
    expected = {ip_address(endpoint[0].split("%", 1)[0]) for *_, endpoint in
                socket.getaddrinfo(settings["hostname"], settings["port"], type=socket.SOCK_STREAM)}
    if user != settings["user"] or port != settings["port"] or ip_address(address) not in expected:
        raise ValueError("已授权的 SSH 控制连接没有指向指定目标")


def control_command(control, action, host):
    return [*SSH, "-S", str(control), "-O", action, "--", host]


def stop_with_signal(number, _frame):
    raise SystemExit(128 + number)


@contextmanager
def file_key_command(host, control, python, argv):
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.@-]*", host) or not argv:
        raise ValueError("临时密钥模式需要 SSH 目标和一条远端命令")
    if not re.fullmatch(r"[A-Za-z0-9_./-]+", python) or python.startswith("-"):
        raise ValueError("远端 Python 入口必须是命令名或路径")
    settings = ssh_settings(host)
    direct = [value for key in DIRECT_OPTIONS if key in settings for value in ("-o", f"{key}={settings[key]}")]
    helper = Path(__file__).with_name("ssh_key_lease_remote.py").read_text(encoding="utf-8")
    tag = f"codex-ssh-once-{uuid4().hex}"
    with TemporaryDirectory(prefix="codex-ssh.") as temporary:
        work = Path(temporary)
        key = work / "key"
        bootstrap_socket = control if control else work / "bootstrap"
        signals = {number: signal.signal(number, stop_with_signal) for number in (signal.SIGHUP, signal.SIGTERM)}
        authorized = False
        try:
            if not control:
                checked("建立初次授权连接", [*SSH, "-fMN", "-S", str(bootstrap_socket), "-o", "ControlPersist=no", "--", host],
                        stdin=subprocess.DEVNULL, timeout=25)
            checked("查验初次授权连接", control_command(bootstrap_socket, "check", host), capture_output=True, timeout=5)
            bootstrap = [*SSH, "-S", str(bootstrap_socket), "-o", "IdentityAgent=none", "-o", "PubkeyAuthentication=no",
                         "-o", "PreferredAuthentications=none", "--", host]
            verify_control(bootstrap, python, settings)
            checked("生成临时 SSH 密钥", ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", tag, "-f", str(key)],
                    stdin=subprocess.DEVNULL, timeout=5)
            payload = json.dumps({"publicKey": key.with_suffix(".pub").read_text().strip(),
                                  "argv": argv, "python": python, "helper": helper})
            authorized = True
            install = shlex.join([python, "-I", "-B", "-c", helper, "install", tag])
            checked("登记临时公钥", [*bootstrap, install], input=payload, text=True, timeout=18)
            yield [*SSH, "-F", "/dev/null", *direct, "-o", "IdentityAgent=none", "-o", "IdentitiesOnly=yes",
                   "-o", "PreferredAuthentications=publickey", "-o", "ControlPath=none", "-i", str(key),
                   "--", host, shlex.join(argv)]
        finally:
            try:
                if authorized:
                    revoke = shlex.join([python, "-I", "-B", "-c", helper, "revoke", tag])
                    try:
                        checked("撤销临时公钥", [*bootstrap, revoke], stdin=subprocess.DEVNULL, timeout=12)
                    except (OSError, RuntimeError) as error:
                        raise RuntimeError("临时公钥撤销未收到回执，公钥最多在登记后五分钟失效") from error
            finally:
                try:
                    if not control and bootstrap_socket.is_socket():
                        subprocess.run(control_command(bootstrap_socket, "exit", host), capture_output=True, timeout=5)
                finally:
                    for number, handler in signals.items():
                        signal.signal(number, handler)


def main():
    parser = argparse.ArgumentParser(description="为一条远端验收命令签发临时 SSH 文件密钥")
    parser.add_argument("--host", required=True)
    parser.add_argument("--ssh-control", type=Path)
    parser.add_argument("--remote-python", default="python3")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    try:
        with file_key_command(args.host, args.ssh_control, args.remote_python, command) as ssh:
            returncode = subprocess.run(ssh).returncode
            return returncode if returncode >= 0 else 128 - returncode
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"ssh-file-key: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
