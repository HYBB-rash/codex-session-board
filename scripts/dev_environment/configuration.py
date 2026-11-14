import hashlib
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys

from scripts.verify_acceptance import fingerprint
from scripts.records import write_record


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def private_directory(path):
    path.mkdir(mode=0o700, exist_ok=True)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError(f"开发目录必须由当前用户独占且不能为符号链接：{path}")


def task_directory(source, create=True):
    parent = Path.home() / ".csb-dev"
    root = parent / hashlib.sha256(os.fsencode(source)).hexdigest()[:12]
    for directory in (parent, root):
        if create or directory.exists() or directory.is_symlink():
            private_directory(directory)
    if len(os.fsencode(root / "data/control.sock")) >= 100:
        raise ValueError("开发目录过长，无法安全创建 macOS Unix socket")
    return root


def source_fingerprint(source):
    bindings = json.loads((source / "acceptance/bindings.json").read_text())
    return fingerprint(source, bindings["fingerprint_paths"])


def child_environment(root):
    environment = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "en_US.UTF-8"}
    locations = {"HOME": root / "home", "CODEX_HOME": root / "codex",
                 "XDG_CONFIG_HOME": root / "config", "XDG_DATA_HOME": root / "share",
                 "XDG_STATE_HOME": root / "state", "XDG_RUNTIME_DIR": root / "runtime",
                 "TMPDIR": root / "tmp"}
    for name, directory in locations.items():
        private_directory(directory)
        environment[name] = str(directory)
    environment["CODEX_SESSION_BOARD_CONFIG"] = str(root / "config.json")
    environment["CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET"] = str(root / "desktop/rpc.sock")
    return environment


def boundary(root):
    names = ("prepared.json", "runtime.json", "action.lock", "owner.lock", "config.json",
             "projects.json", "fixture.json", "navigation.jsonl", "fixture.log",
             "service.log", "supervisor.log", "data", "desktop", "sidebar")
    for name in names:
        path = root / name
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"开发路径越界：{name}")
    data = root / "data"
    if data.exists() and any(path.is_symlink() for path in data.rglob("*")):
        raise ValueError("开发数据库目录不能包含符号链接")


def configuration(source, root, manifest):
    boundary(root)
    config = json.loads((root / "config.json").read_text())
    expected = {
        "data_dir": str(root / "data"), "desktop_socket": str(root / "desktop/rpc.sock"),
        "sidebar_socket": str(root / "sidebar/rpc.sock"), "sidebar_state_file": str(root / "projects.json"),
        "codex_command": [manifest["fixture"], "history", str(root)],
        "open_command": [manifest["fixture"], "open", str(root)],
        "ssh_command": [manifest["fixture"], "deny", str(root)],
        "proxy_command": [], "clock_command": [], "host_id": "local",
        "native_sections": True, "legacy_mode": False, "sync_sections": False,
        "auto_archive_seconds": None, "remote_auto_archive_seconds": None,
    }
    if manifest["source"] != str(source) or any(config.get(key) != value for key, value in expected.items()):
        raise ValueError("开发配置越过本任务边界，拒绝启动")
    for name in ("config.json", "projects.json"):
        if digest(root / name) != manifest["files"][name]:
            raise ValueError(f"开发配置已被修改，请显式重新 setup：{name}")
    return config


def load(source, root):
    manifest = json.loads((root / "prepared.json").read_text())
    configuration(source, root, manifest)
    for name, relative in (("program", "debug/codex-session-board"), ("fixture", "debug/examples/dev_fixture")):
        if manifest[name] != str(source / "target" / relative) or digest(manifest[name]) != manifest[name + "_sha256"]:
            raise ValueError("开发可执行文件身份变化，请重新 setup")
    return manifest


def configure(source, root, manifest):
    boundary(root)
    with socket.socket() as lease:
        lease.bind(("127.0.0.1", 0))
        port = lease.getsockname()[1]
    subprocess.run([manifest["fixture"], "prepare", str(root), str(port)], cwd=source,
                   env=child_environment(root), check=True)
    manifest["files"] = {name: digest(root / name) for name in ("config.json", "projects.json")}
    write_record(root / "prepared.json", manifest)
    configuration(source, root, manifest)
    return manifest


def setup(source, root):
    target = source / "target"
    if target.is_symlink():
        raise ValueError("开发构建目录不能是共享符号链接")
    before = source_fingerprint(source)
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_CACHE_AUTO_CLEAN_FREQUENCY="never")
    if sys.platform == "darwin":
        environment["TMPDIR"] = "/private/tmp"
    subprocess.run(["cargo", "fetch", "--locked"], cwd=source, env=environment, check=True,
                   stdout=sys.stderr)
    subprocess.run(["cargo", "build", "--locked", "--offline", "--bin", "codex-session-board",
                    "--example", "dev_fixture"], cwd=source, env=environment, check=True,
                   stdout=sys.stderr)
    if source_fingerprint(source) != before:
        raise ValueError("构建期间源码变化，请重新 setup")
    program = target / "debug/codex-session-board"
    fixture = target / "debug/examples/dev_fixture"
    return configure(source, root, {
        "source": str(source), "source_sha": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=source, text=True).strip(),
        "fingerprint": before, "program": str(program), "fixture": str(fixture),
        "program_sha256": digest(program), "fixture_sha256": digest(fixture),
    })
