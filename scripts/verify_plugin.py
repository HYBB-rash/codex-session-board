import argparse
from contextlib import closing
from datetime import datetime, timezone
import hashlib
from http.client import HTTPConnection
import json
import os
from pathlib import Path
import subprocess
import time

from package_plugin import verify_package
from update_plugin import cache_directory, marketplace_file, write_record
from reload_plugins import reload_plugins


ROOT = Path(__file__).resolve().parents[1]


def process_identity(pid):
    value = subprocess.check_output(
        ["ps", "-p", str(pid), "-o", "uid=,ppid=,lstart=,args="],
        text=True, timeout=5).strip().split(None, 7)
    if len(value) != 8 or int(value[0]) != os.getuid():
        raise ValueError("Process is missing or belongs to another user")
    executable = Path(f"/proc/{pid}/exe")
    if executable.exists():
        executable = executable.resolve()
    else:
        listing = subprocess.check_output(
            ["/usr/sbin/lsof", "-a", "-p", str(pid), "-d", "txt", "-Fn"],
            text=True, timeout=5)
        executable = Path(next(line[1:] for line in listing.splitlines() if line.startswith("n")))
    return {"pid": pid, "parent_pid": int(value[1]), "started": " ".join(value[2:7]),
            "command": value[7], "executable": str(executable.resolve())}


def runtime_info(port):
    with closing(HTTPConnection("127.0.0.1", port, timeout=5)) as connection:
        connection.request("GET", "/api/info")
        response = connection.getresponse()
        if response.status != 200:
            raise ValueError(f"Runtime identity returned HTTP {response.status}")
        result = json.load(response)
        if not isinstance(result, dict):
            raise ValueError("Runtime identity must be a JSON object")
        return result


def inspect(state, config_path, client_pid, expected_source=None, config_id=None):
    original = config_path.read_bytes()
    config = json.loads(original)
    record = json.loads((state / "current.json").read_text())
    if record.get("installed") is not True:
        raise ValueError("No completed installation")
    manifest = verify_package(Path(record["package"]))
    if manifest != record["release"]:
        raise ValueError("Installation receipt differs from the verified package")
    if expected_source and manifest["source_commit"] != expected_source:
        raise ValueError("Installed source differs from the requested release")
    cached = cache_directory(manifest["version"])
    if verify_package(cached, installed=True) != manifest:
        raise ValueError("Client plugin cache differs from the release")
    if not isinstance(config.get("data_dir"), str) or not Path(config["data_dir"]).is_absolute():
        raise ValueError("Configuration requires an explicit absolute data directory")
    data_dir = Path(config["data_dir"]).resolve()
    port = config["port"]
    if type(port) is not int or not 0 < port < 65536:
        raise ValueError("Configuration requires an explicit valid loopback port")
    result = {
        "phase": "awaiting_desktop_reload", "passed": False, "desktop_loaded": False,
        "source_commit": manifest["source_commit"], "version": manifest["version"],
        "binary_sha256": manifest["binary_sha256"], "revision": record["revision"],
        "url": f"http://127.0.0.1:{port}", "data_dir": str(data_dir),
        "config": str(config_path), "config_sha256": hashlib.sha256(original).hexdigest(),
        "installation_state": str(state.resolve()),
        "client_pid": client_pid, "package_verified": True,
    }
    try:
        info = runtime_info(port)
    except OSError as error:
        return {**result, "detail": str(error)}
    result["runtime"] = info
    if info.get("app") != "codex-session-board" or info.get("dataDir") != str(data_dir):
        raise ValueError("Runtime application or data directory differs")
    if (info.get("version") != manifest["version"] or info.get("build") != manifest["binary_sha256"]):
        return {**result, "detail": "Installed package is not the running release"}
    if config_id is not None and info.get("configId") != config_id:
        raise ValueError("Runtime configuration identity differs")
    pid = info.get("pid")
    if type(pid) is not int or pid <= 0:
        raise ValueError("Runtime PID is invalid")
    runtime = process_identity(pid)
    client = process_identity(client_pid)
    if runtime["parent_pid"] != client_pid or " app-server" not in client["command"]:
        return {**result, "detail": "Runtime is not owned by the selected client",
                "runtime_process": runtime}
    binary = cached / "bin/codex-session-board"
    if Path(runtime["executable"]) != binary.resolve() or not runtime["command"].endswith(" mcp"):
        raise ValueError("Selected client is not running the verified installed MCP executable")
    if hashlib.sha256(binary.read_bytes()).hexdigest() != info["build"]:
        raise ValueError("Runtime executable identity differs")
    if (config_path.read_bytes() != original or process_identity(pid) != runtime
            or process_identity(client_pid) != client or runtime_info(port) != info):
        raise ValueError("Runtime, client or configuration changed during verification")
    return {**result, "phase": "desktop_loaded", "passed": True, "desktop_loaded": True,
            "runtime_process": runtime, "client_process": client,
            "runtime_config_id": info["configId"], "expected_config_id_checked": config_id is not None}


def main():
    parser = argparse.ArgumentParser()
    data_home = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    config_home = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
    parser.add_argument("--state", type=Path, default=data_home / "codex-session-board/installation")
    parser.add_argument("--config", type=Path, default=config_home / "codex-session-board/config.json")
    parser.add_argument("--client-pid", required=True, type=int)
    parser.add_argument("--expected-source")
    parser.add_argument("--config-id")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--reconnect", action="store_true")
    parser.add_argument("--codex", default="codex")
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if output.is_relative_to(ROOT) or output.exists() or arguments.client_pid <= 0:
        parser.error("Use a new report outside the source and a positive client PID")
    output.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    result = {"passed": False, "phase": "failed", "desktop_loaded": False,
              "observed_at": datetime.now(timezone.utc).isoformat()}
    try:
        if arguments.reconnect:
            record = json.loads((arguments.state / "current.json").read_text())
            result["reconnect"] = reload_plugins(
                arguments.codex, arguments.state, record["release"]["version"],
                marketplace_file(arguments.codex))
        result.update(inspect(arguments.state, arguments.config.resolve(), arguments.client_pid,
                              arguments.expected_source, arguments.config_id))
    except (OSError, ValueError, KeyError, RuntimeError, ImportError, StopIteration, subprocess.SubprocessError) as error:
        result.update(passed=False, phase="failed", error=str(error))
    result["seconds"] = time.monotonic() - started
    write_record(output, result)
    print(json.dumps({"phase": result["phase"], "seconds": result["seconds"], "report": str(output)}))
    return 0 if result["passed"] else 2 if result["phase"] == "awaiting_desktop_reload" else 1


if __name__ == "__main__":
    raise SystemExit(main())
