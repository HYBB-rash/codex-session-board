import fcntl
from datetime import datetime, timedelta, timezone
from hashlib import sha256
from ipaddress import ip_address
import json
import os
from pathlib import Path
import re
import shlex
import stat
import sys


def update_keys(tag, public=None, command=None):
    home = Path.home() / ".ssh"
    directory = home.lstat()
    if (not stat.S_ISDIR(directory.st_mode) or directory.st_uid != os.getuid()
            or stat.S_IMODE(directory.st_mode) & 0o022):
        raise ValueError("SSH 授权目录归属不符")
    keys = home / "authorized_keys"
    with os.fdopen(os.open(keys, os.O_RDWR | os.O_NOFOLLOW), "r+b") as handle:
        metadata = os.fstat(handle.fileno())
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
                or stat.S_IMODE(metadata.st_mode) & 0o022):
            raise ValueError("authorized_keys 不是当前用户拥有的私有普通文件")
        fcntl.flock(handle, fcntl.LOCK_EX)
        current = handle.read()
        lines = current.splitlines(keepends=True)
        marker = (" " + tag).encode()
        owned = [line for line in lines if line.rstrip(b"\r\n").endswith(marker)]
        if public is not None:
            if owned or current and not current.endswith(b"\n"):
                raise ValueError("authorized_keys 缺少末尾换行或已有本次授权")
            origin = ip_address(os.environ["SSH_CONNECTION"].split()[0])
            expires = (datetime.now(timezone.utc) + timedelta(minutes=5)).strftime("%Y%m%d%H%M%SZ")
            escaped = command.replace("\\", "\\\\").replace('"', '\\"')
            entry = f'restrict,from="{origin}",expiry-time="{expires}",command="{escaped}" {public}\n'
            handle.write(entry.encode())
        elif owned:
            restored = b"".join(line for line in lines if line not in owned)
            handle.seek(0)
            handle.write(restored)
            handle.truncate()
        else:
            return
        handle.flush()
        os.fsync(handle.fileno())


def revoke(directory, tag):
    update_keys(tag)
    if directory.is_symlink():
        raise ValueError("SSH 授权暂存目录不能是符号链接")
    if directory.is_dir():
        (directory / "command.json").unlink(missing_ok=True)
        (directory / "claimed.json").unlink(missing_ok=True)
        (directory / "lease.py").unlink(missing_ok=True)
        directory.rmdir()


def install(directory, tag):
    data = json.load(sys.stdin)
    public, argv, python = data["publicKey"], data["argv"], data["python"]
    if not re.fullmatch(r"ssh-ed25519 [A-Za-z0-9+/=]+ " + tag, public):
        raise ValueError("公钥格式或本次授权标记不符")
    directory.mkdir(mode=0o700)
    try:
        script = directory / "lease.py"
        script.write_text(data["helper"])
        (directory / "command.json").write_text(json.dumps(argv))
        command = shlex.join([python, "-I", "-B", str(script), "run", tag])
        update_keys(tag, public, command)
    except BaseException:
        revoke(directory, tag)
        raise


def run(directory, tag):
    claimed = (directory / "command.json").rename(directory / "claimed.json")
    argv = json.loads(claimed.read_text())
    expected = sha256(shlex.join(argv).encode()).digest()
    original = sha256(os.environ.get("SSH_ORIGINAL_COMMAND", "").encode()).digest()
    revoke(directory, tag)
    if original != expected:
        raise ValueError("临时 SSH 授权只允许本次指定的命令")
    os.execvp(argv[0], argv)


def main():
    action, tag = sys.argv[1:]
    if not re.fullmatch(r"codex-ssh-once-[a-f0-9]{32}", tag):
        raise ValueError("临时 SSH 授权标记无效")
    directory = Path("/tmp") / f"{tag}-{os.getuid()}"
    if action == "install":
        install(directory, tag)
    elif action == "run":
        run(directory, tag)
    elif action == "revoke":
        revoke(directory, tag)
    else:
        raise ValueError("临时 SSH 授权操作无效")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"临时 SSH 授权：{error}", file=sys.stderr)
        sys.exit(1)
