#!/usr/bin/env python3
import argparse
from contextlib import contextmanager
import os
from pathlib import Path
import re
import secrets
import select
import shlex
import signal
import subprocess
import sys
import tempfile
from urllib.parse import quote


def port_mapping(value):
    ports = value.split(":")
    if len(ports) not in (1, 2) or any(
        not port.isascii() or not port.isdecimal() or not 1 <= int(port) <= 65535
        for port in ports
    ):
        raise argparse.ArgumentTypeError("expected PORT or LOCAL:REMOTE (1–65535)")
    return int(ports[0]), int(ports[-1])


def parse_args(argv):
    parser = argparse.ArgumentParser(
        description="Run a GUI test on a private Linux display or an SSH target.",
        usage="%(prog)s [--host SSH_TARGET] [options] -- COMMAND [ARG ...]",
    )
    parser.add_argument("--host", help="SSH alias or user@host")
    parser.add_argument("--ssh-control", type=Path, help="use an already authorized SSH control connection")
    parser.add_argument("--remote-python", default="python3", help="Python executable on the SSH target")
    parser.add_argument("--temporary-key", action="store_true", help="use a one-shot SSH file key")
    parser.add_argument("--cwd", help="working directory on the test host")
    parser.add_argument("--forward", action="append", type=port_mapping, default=[],
                        help="forward a loopback TCP port, PORT or LOCAL:REMOTE (repeatable)")
    parser.add_argument("--screen", metavar="WIDTHxHEIGHT", help="Linux screen (default: 1600x1000)")
    parser.add_argument("--timeout", type=int, default=900, metavar="SECONDS",
                        help="maximum command runtime (default: 900)")
    boundary = argv.index("--") if "--" in argv else len(argv)
    options = parser.parse_args(argv[:boundary])
    command = argv[boundary + 1:]
    if not command:
        parser.error("provide a command after --")
    if options.host and not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.@-]*", options.host):
        parser.error("--host expects an SSH alias or user@host")
    if options.forward and not options.host:
        parser.error("--forward requires --host")
    if (options.temporary_key or options.ssh_control) and not options.host:
        parser.error("--temporary-key and --ssh-control require --host")
    if options.temporary_key and options.forward:
        parser.error("--temporary-key uses a command-only key and does not support --forward")
    if options.screen and not re.fullmatch(r"[1-9][0-9]{0,3}x[1-9][0-9]{0,3}", options.screen):
        parser.error("--screen expects WIDTHxHEIGHT")
    if options.timeout <= 0:
        parser.error("--timeout must be positive")
    return options, command


def run_over_ssh(options, command):
    remote = [options.remote_python, "-B", "-c", Path(__file__).read_text(encoding="utf-8"),
              "--timeout", str(options.timeout)]
    if options.cwd is not None:
        remote.extend(["--cwd", options.cwd])
    if options.screen is not None:
        remote.extend(["--screen", options.screen])
    remote.extend(["--", *command])
    if options.temporary_key:
        from ssh_file_key import file_key_command
        with file_key_command(options.host, options.ssh_control, options.remote_python, remote) as ssh:
            status = subprocess.run(ssh).returncode
            return status if status >= 0 else 128 - status
    ssh = [
        "ssh", "-T", "-x", "-a", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
        "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=10", "-o", "ServerAliveCountMax=3",
        "-o", "ExitOnForwardFailure=yes",
    ]
    for local, target in options.forward:
        ssh.extend(["-L", f"127.0.0.1:{local}:127.0.0.1:{target}"])
    if options.ssh_control:
        ssh.extend(["-S", str(options.ssh_control)])
    os.execvp(ssh[0], [*ssh, "--", options.host, shlex.join(remote)])


def terminate(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


@contextmanager
def private_display(directory, screen):
    authority = directory / "Xauthority"
    authority.touch(mode=0o600)
    cookie = secrets.token_hex(16)

    def authorize(display):
        subprocess.run(
            ["xauth", "-q", "-f", str(authority), "source", "-"],
            input=f"add {display} . {cookie}\n", text=True, check=True,
        )

    authorize(":0")
    reader, writer = os.pipe()
    log = directory / "Xvfb.log"
    with os.fdopen(reader, "rb") as ready, log.open("wb") as errors:
        try:
            server = subprocess.Popen(
                ["Xvfb", "-displayfd", str(writer), "-screen", "0", f"{screen}x24",
                 "-auth", str(authority), "-nolisten", "tcp", "-noreset"],
                stderr=errors, pass_fds=(writer,), start_new_session=True,
            )
        finally:
            os.close(writer)
        try:
            if not select.select([ready], [], [], 10)[0]:
                raise RuntimeError(f"Xvfb did not report a display within 10 seconds\n{log.read_text()}")
            number = ready.readline(32).strip()
            if not number.isdigit():
                raise RuntimeError(f"Xvfb exited without reporting a display\n{log.read_text()}")
            display = ":" + number.decode("ascii")
            authorize(display)
            yield display, authority
        finally:
            terminate(server)


def run_command(command, env, options):
    process = subprocess.Popen(
        command, cwd=os.path.expanduser(options.cwd) if options.cwd else None, env=env,
        start_new_session=True,
    )
    try:
        status = process.wait(timeout=options.timeout)
        return status if status >= 0 else 128 - status
    except subprocess.TimeoutExpired:
        print(f"gui-test: command exceeded {options.timeout} seconds", file=sys.stderr)
        return 124
    finally:
        terminate(process)


def private_bus(runtime):
    config = runtime / "dbus.conf"
    address = quote(str(runtime / "bus"), safe="/")
    config.write_text(
        f"""<busconfig>
  <type>session</type>
  <listen>unix:path={address}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
""", encoding="utf-8",
    )
    return ["dbus-run-session", f"--config-file={config}", "--"]


def run_here(options, command, directory):
    env = {**os.environ, "GUI_TEST_TMPDIR": str(directory)}
    if sys.platform == "linux":
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
            env.pop(key, None)
        runtime = directory / "runtime"
        runtime.mkdir(mode=0o700)
        with private_display(directory, options.screen or "1600x1000") as (display, authority):
            env.update(DISPLAY=display, XAUTHORITY=str(authority), XDG_RUNTIME_DIR=str(runtime),
                       XDG_SESSION_TYPE="x11", GDK_BACKEND="x11", QT_QPA_PLATFORM="xcb")
            return run_command([*private_bus(runtime), *command], env, options)
    if sys.platform == "darwin":
        if options.screen:
            raise RuntimeError("--screen applies to Linux Xvfb")
        aqua = subprocess.run(
            ["/bin/launchctl", "print", f"gui/{os.getuid()}"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5,
        )
        if aqua.returncode:
            raise RuntimeError("the macOS test account needs an existing graphical login")
        return run_command(command, env, options)
    raise RuntimeError(f"unsupported test host: {sys.platform}")


def interrupted(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    options, command = parse_args(sys.argv[1:])
    try:
        if options.host:
            return run_over_ssh(options, command)
        for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(signum, interrupted)
        with tempfile.TemporaryDirectory(prefix="gui-test-") as work:
            return run_here(options, command, Path(work))
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"gui-test: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
