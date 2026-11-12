import fcntl
from contextlib import closing
from http.client import HTTPConnection
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import stat
import subprocess
import sys
import threading
import time
from urllib.parse import urlsplit

from scripts.dev_environment import configuration as config
from scripts.dev_environment.processes import Child, reap


def public(record):
    return {key: value for key, value in record.items() if key != "_token"}


def request(root, action):
    path = root / "supervisor.sock"
    metadata = path.lstat()
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError("开发控制 socket 不属于当前任务")
    record = json.loads((root / "runtime.json").read_text())
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(12)
        connection.connect(str(path))
        connection.sendall((json.dumps({"action": action, "token": record["_token"]}) + "\n").encode())
        line = connection.makefile("r").readline(65537)
        if not line:
            raise ConnectionError("开发控制连接已关闭")
        response = json.loads(line)
    if response.get("error"):
        raise ValueError(response["error"])
    return response


def status(root):
    try:
        return request(root, "status")
    except (FileNotFoundError, ConnectionError):
        path = root / "runtime.json"
        prepared = (root / "prepared.json").exists()
        record = json.loads(path.read_text()) if path.exists() else {"phase": "prepared" if prepared else "unprepared"}
        if record["phase"] in ("starting", "running") or record.get("cleanup_confirmed") is False:
            record = {**record, "phase": "unreachable"}
        return public(record)


def clear_socket(path):
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid():
        raise ValueError(f"不能清理未知文件：{path}")
    with socket.socket(socket.AF_UNIX) as probe:
        probe.settimeout(0.3)
        try:
            probe.connect(str(path))
        except (ConnectionRefusedError, FileNotFoundError):
            path.unlink(missing_ok=True)
            return
    raise ValueError(f"已有监听者，拒绝接管：{path}")


def probe(record):
    address = urlsplit(record["url"])
    with closing(HTTPConnection(address.hostname, address.port, timeout=0.8)) as connection:
        connection.connect()
        connection.auto_open = False
        pinned = connection.sock

        def read(path):
            if connection.sock is not pinned:
                raise RuntimeError("身份连接已关闭，拒绝重新连接读取会话")
            connection.request("GET", path)
            with connection.getresponse() as response:
                if response.status != 200:
                    raise RuntimeError(f"开发服务返回 HTTP {response.status}，不跟随重定向")
                value = json.load(response)
            if connection.sock is not pinned:
                raise RuntimeError("身份连接已关闭，拒绝继续读取会话")
            return value

        info = read("/api/info")
        if (info.get("pid") != record["service_pid"] or info.get("build") != record["program_sha256"]
                or info.get("dataDir") != str(Path(record["root"]) / "data")):
            raise RuntimeError("服务身份不匹配，未读取会话")
        snapshot = read("/api/sessions")
    health = snapshot["health"]
    errors = [health.get(key) for key in ("error", "runtimeError", "nativeError", "nativeStageError", "sectionsError")]
    errors.extend(session.get("observationError") for session in snapshot["sessions"])
    verified = all(health.get(key) is True for key in ("scanComplete", "runtimeVerified", "nativeVerified"))
    if not verified or any(errors) or not snapshot["projectCatalog"]["loaded"]:
        raise RuntimeError(f"本任务原生依赖尚未就绪：{[error for error in errors if error]}")


def ready(children, record, cancelled):
    deadline = time.monotonic() + 30
    detail = "尚未监听"
    while time.monotonic() < deadline and not cancelled.is_set():
        if any(child.poll() is not None for child in children):
            raise RuntimeError("开发子进程提前退出，请查看本任务日志")
        try:
            record["service_pid"] = children[1].pid
            probe(record)
            return
        except (OSError, ValueError, KeyError, RuntimeError) as error:
            detail = str(error)
        cancelled.wait(0.15)
    raise RuntimeError(f"开发服务启动超时或取消：{detail}")


def serve(source, root):
    with (root / "owner.lock").open("a") as owner:
        fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
        supervise(source, root)


def supervise(source, root):
    manifest = config.load(source, root)
    settings = config.configuration(source, root, manifest)
    for path in (root / "supervisor.sock", root / "desktop/rpc.sock", root / "sidebar/rpc.sock"):
        clear_socket(path)
    cancelled = threading.Event()
    for number in (signal.SIGINT, signal.SIGTERM):
        signal.signal(number, lambda *_: cancelled.set())
    record = {**manifest, "root": str(root), "phase": "starting", "cleanup_confirmed": False,
              "url": f"http://127.0.0.1:{settings['port']}", "_token": secrets.token_hex(24)}
    config.write_record(root / "runtime.json", record)
    children = []
    server, stopping, failure, bound = None, None, None, False
    try:
        environment = config.child_environment(root)
        commands = ([manifest["fixture"], "serve", str(root)], [manifest["program"], "--config", str(root / "config.json"), "serve"])
        for name, command in zip(("fixture", "service"), commands):
            children.append(Child(root, name, command, environment))
        ready(children, record, cancelled)
        record.update(phase="running", fixture_pid=children[0].pid, service_pid=children[1].pid)
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(root / "supervisor.sock"))
        bound = True
        os.chmod(root / "supervisor.sock", 0o600)
        server.listen(4)
        server.settimeout(0.3)
        config.write_record(root / "runtime.json", record)
        while not cancelled.is_set():
            if any(child.poll() is not None for child in children):
                raise RuntimeError("开发依赖退出，已关闭本任务环境")
            try:
                connection, _ = server.accept()
            except TimeoutError:
                continue
            connection.settimeout(2)
            try:
                message = json.loads(connection.makefile("r").readline(4097))
                if (not isinstance(message, dict) or not isinstance(message.get("token"), str)
                        or not isinstance(message.get("action"), str)):
                    raise ValueError("无效控制请求")
                if not secrets.compare_digest(message["token"].encode(), record["_token"].encode()):
                    raise ValueError("控制凭据不匹配")
                if message.get("action") == "stop":
                    stopping = connection
                    break
                if message.get("action") != "status":
                    raise ValueError("未知控制操作")
                current = public(record)
                try:
                    probe(record)
                except (OSError, ValueError, KeyError, RuntimeError) as error:
                    current.update(phase="degraded", reason=str(error))
                connection.sendall((json.dumps(current) + "\n").encode())
            except (OSError, ValueError, RecursionError) as error:
                try:
                    connection.sendall((json.dumps({"error": str(error)}) + "\n").encode())
                except OSError:
                    pass
            finally:
                if stopping is not connection:
                    connection.close()
    except BaseException as error:
        failure = str(error)
    finally:
        try:
            reap(children)
            record["cleanup_confirmed"] = True
        except RuntimeError as error:
            failure = f"{failure or ''}；清理未完成：{error}"
        if server:
            server.close()
        if bound:
            (root / "supervisor.sock").unlink(missing_ok=True)
        record.update(phase="failed" if failure is not None else "stopped", reason=failure)
        config.write_record(root / "runtime.json", record)
        if stopping:
            try:
                stopping.sendall((json.dumps(public(record)) + "\n").encode())
            finally:
                stopping.close()
    if failure is not None:
        raise RuntimeError(failure)


def setup(source, root):
    existing = status(root)
    if existing["phase"] == "running":
        manifest = config.load(source, root)
        if config.source_fingerprint(source) != manifest["fingerprint"]:
            raise ValueError("源码已变化，请先 stop 再 setup")
        return existing
    if existing["phase"] in ("unreachable", "degraded"):
        raise ValueError("已有环境尚未确认退出，拒绝覆盖配置")
    return {"phase": "prepared", "root": str(root), **config.setup(source, root)}


def start(source, root):
    existing = status(root)
    if existing["phase"] == "running":
        setup(source, root)
        return existing
    if existing["phase"] in ("unreachable", "degraded"):
        raise ValueError("已有环境无法联系，拒绝重复启动；请检查本任务日志")
    if not (root / "prepared.json").exists():
        config.setup(source, root)
    manifest = config.load(source, root)
    if config.source_fingerprint(source) != manifest["fingerprint"]:
        raise ValueError("源码已变化，请先运行 ./scripts/dev setup")
    for _ in range(3):
        with (root / "supervisor.log").open("ab") as log:
            process = subprocess.Popen([sys.executable, str(source / "scripts/dev"), "_run"], cwd=source,
                                       stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
        deadline = time.monotonic() + 35
        while process.poll() is None and time.monotonic() < deadline:
            result = status(root)
            if result["phase"] == "running":
                return result
            time.sleep(0.15)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=12)
        result = status(root)
        if result["phase"] not in ("stopped", "failed"):
            raise RuntimeError("启动未能确认清理，拒绝继续重试")
        config.configure(source, root, manifest)
    raise RuntimeError(f"开发环境启动失败：{result.get('reason')}；日志在 {root}")


def stop(root):
    existing = status(root)
    if existing["phase"] in ("running", "degraded"):
        result = request(root, "stop")
        if result["phase"] != "stopped" or not result.get("cleanup_confirmed"):
            raise RuntimeError(f"停止未能确认完成：{result.get('reason')}")
        return result
    if existing["phase"] == "unreachable":
        raise ValueError("控制连接不可用，不会根据旧 PID 杀进程")
    return existing
