import argparse
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time


def endpoint_pid(path):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(1)
        connection.connect(str(path))
        if sys.platform == "darwin":
            return struct.unpack("i", connection.getsockopt(0, 2, 4))[0]
        return struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[0]


def wait_backend(process, endpoint, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("共享后台提前退出")
        try:
            if endpoint_pid(endpoint) != process.pid:
                raise RuntimeError("共享端点不属于本次启动的后台")
            return
        except (FileNotFoundError, ConnectionRefusedError):
            time.sleep(0.05)
    raise TimeoutError("共享后台未就绪")


def wait_desktop(process, log, version, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("客户端退出或被已有客户端接管，未连接本次共享后台")
        text = log.read_text(errors="replace")
        if "stdio_transport_spawned" in text:
            raise RuntimeError("客户端另起了 stdio 后台")
        if (f"currentVersion={version} hostId=local" in text
                and "outcome=success transportKind=websocket" in text
                and "next=connected" in text):
            return
        time.sleep(0.1)
    raise TimeoutError("客户端未确认连接共享后台")


def stop(process):
    if process is None:
        return
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    else:
        process.wait()


def launch(app, backend, home, profile, report_path=None, probe=False, timeout=35):
    root = Path(tempfile.mkdtemp(prefix="csb-shared-", dir="/private/tmp" if sys.platform == "darwin" else None))
    endpoint = root / "rpc.sock"
    report = {"passed": False, "root": str(root), "endpoint": str(endpoint)}
    environment = dict(os.environ)
    for name in ("CODEX_CLI_PATH", "CODEX_APP_SERVER_FORCE_CLI", "CODEX_APP_SERVER_USE_LOCAL_DAEMON"):
        environment.pop(name, None)
    environment.update(CODEX_HOME=str(home), CODEX_ELECTRON_USER_DATA_PATH=str(profile),
                       CODEX_APP_SERVER_WS_URL=f"ws+unix://localhost{endpoint}:/rpc",
                       CODEX_SPARKLE_ENABLED="false")
    server, desktop = None, None
    try:
        version = subprocess.check_output([str(backend), "--version"], env=environment,
                                          text=True, timeout=10).strip().removeprefix("codex-cli ")
        report["version"] = version
        with (root / "backend.log").open("w") as backend_log, (root / "desktop.log").open("w") as desktop_log:
            server = subprocess.Popen([str(backend), "app-server", "--listen", f"unix://{endpoint}"],
                                      env=environment, stdout=backend_log, stderr=backend_log)
            report["backend_pid"] = server.pid
            wait_backend(server, endpoint, timeout)
            desktop = subprocess.Popen([str(app), f"--user-data-dir={profile}", "--disable-background-networking"],
                                       env=environment, stdout=desktop_log, stderr=desktop_log)
            report["desktop_pid"] = desktop.pid
            wait_desktop(desktop, root / "desktop.log", version, timeout)
            report["endpoint_peer_pid"] = endpoint_pid(endpoint)
            if report["endpoint_peer_pid"] != server.pid:
                raise RuntimeError("客户端就绪后共享后台归属发生变化")
            report.update(passed=True, phase="connected")
            print(json.dumps(report, ensure_ascii=False), flush=True)
            if not probe:
                while desktop.poll() is None:
                    if server.poll() is not None:
                        raise RuntimeError("共享后台退出，已停止本次客户端")
                    time.sleep(0.2)
    except BaseException as error:
        report.update(passed=False, error=str(error))
        raise
    finally:
        try:
            stop(desktop)
        finally:
            stop(server)
            report["cleanup_confirmed"] = all(process is None or process.poll() is not None
                                              for process in (desktop, server))
            report["phase"] = "stopped"
            output = report_path or root / "report.json"
            output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description="让 Mac 客户端和看板共享同一后台，并自动核对连接")
    parser.add_argument("--app", type=Path, default=Path("/Applications/ChatGPT.app"))
    parser.add_argument("--backend", type=Path)
    parser.add_argument("--home", type=Path)
    parser.add_argument("--profile", type=Path)
    parser.add_argument("--probe", action="store_true", help="使用私有环境自动验证，随后停止")
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    app = arguments.app / "Contents/MacOS/ChatGPT"
    backend = arguments.backend or arguments.app / "Contents/Resources/codex-cli/bin/codex"
    if arguments.probe:
        if arguments.home or arguments.profile:
            parser.error("--probe 自动创建私有环境，不接受现有 home/profile")
        root = Path(tempfile.mkdtemp(prefix="csb-client-", dir="/private/tmp" if sys.platform == "darwin" else None))
        home, profile = root / "home", root / "profile"
        home.mkdir(mode=0o700)
        profile.mkdir(mode=0o700)
        (home / "config.toml").write_text('cli_auth_credentials_store = "file"\n')
    else:
        if arguments.home is None or arguments.profile is None:
            parser.error("正常启动需指定 --home 和 --profile；先退出使用该 profile 的客户端")
        home, profile = arguments.home.resolve(), arguments.profile.resolve()
        if not home.is_dir() or not profile.is_dir():
            parser.error("正常启动只接受已存在的 home/profile 目录")
    for number in (signal.SIGINT, signal.SIGTERM):
        signal.signal(number, lambda *_: sys.exit(130))
    report = launch(app.resolve(), backend.resolve(), home, profile, arguments.report, arguments.probe)
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError, TimeoutError) as error:
        print(f"共享客户端启动失败：{error}", file=sys.stderr)
        sys.exit(1)
