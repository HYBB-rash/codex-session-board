import argparse
from contextlib import closing
from datetime import datetime, timezone
from http.client import HTTPConnection
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from scripts.dev_environment.configuration import child_environment, digest
from scripts.dev_environment.processes import Child, reap
from scripts.dev_environment.runtime import ready
from scripts.package_plugin import ROOT, verify_package

CHECKS = [
    "package_hashes", "http_identity", "fixture_ready", "mcp_initialize",
    "mcp_resource", "mcp_snapshot", "cleanup",
]
IDENTITY_FIELDS = ("app", "version", "build", "dataDir", "configId", "pid", "authority")
WORK_STATES = {"in_progress", "review", "blocked", "to_archive"}
UNPLACED_THREAD = "11111111-1111-4111-8111-000000000001"


def remaining_time(deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("Packaged MCP startup exceeded 30 seconds")
    return remaining


def confirm_mcp_exit(root, runtime, deadline):
    exit_deadline = min(deadline, time.monotonic() + 3)
    while True:
        remaining_time(deadline)
        if time.monotonic() >= exit_deadline:
            raise RuntimeError("Packaged MCP exit was not confirmed")
        child = json.loads((root / "mcp-child.json").read_text())
        if child["exit"] is not None:
            remaining_time(deadline)
            break
        time.sleep(min(0.01, remaining_time(deadline)))
    if child["exit"] != 0 or child["parent"] != runtime["authority"]["pid"]:
        raise ValueError("Packaged MCP exited unsuccessfully or belongs to another backend")


def four_column_snapshot(snapshot):
    if snapshot.get("fourColumns") is not True or set(snapshot["states"]) != WORK_STATES:
        raise ValueError("Packaged board did not publish exactly four work columns")
    if set(snapshot["columnSorts"]) != WORK_STATES:
        raise ValueError("Packaged board exposed a retired sorting destination")
    row = next((row for row in snapshot["sessions"] if row["id"] == UNPLACED_THREAD), None)
    if row is None or row["state"] is not None or not row["sectionVersion"]:
        raise ValueError("Packaged board lost the old section's unplaced session")


def mcp(root, version, runtime, four_columns=False):
    deadline = time.monotonic() + 30
    requests = [
        {"id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "package-startup", "version": "1"},
        }},
        {"method": "notifications/initialized"},
        {"id": 2, "method": "resources/read", "params": {"uri": "ui://session-board/index.html"}},
        {"id": 3, "method": "tools/call", "params": {"name": "board_snapshot", "arguments": {}}},
    ]
    try:
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(remaining_time(deadline))
            connection.connect(str(root / "mcp.sock"))
            connection.settimeout(remaining_time(deadline))
            connection.sendall("".join(json.dumps({"jsonrpc": "2.0", **request}) + "\n"
                                       for request in requests).encode())
            connection.shutdown(socket.SHUT_WR)
            output = bytearray()
            while True:
                connection.settimeout(remaining_time(deadline))
                chunk = connection.recv(65536)
                if not chunk:
                    break
                output.extend(chunk)
            replies = [json.loads(line) for line in output.decode().splitlines()]
    except (BrokenPipeError, ConnectionResetError):
        confirm_mcp_exit(root, runtime, deadline)
        raise
    confirm_mcp_exit(root, runtime, deadline)
    if [reply.get("id") for reply in replies] != [1, 2, 3] or any("error" in reply for reply in replies):
        raise ValueError("Packaged MCP did not answer all startup requests")
    initialized, resource, snapshot = [reply["result"] for reply in replies]
    if initialized["serverInfo"] != {"name": "codex-session-board", "version": version}:
        raise ValueError("Packaged MCP version differs")
    content = resource["contents"][0]
    if content["mimeType"] != "text/html;profile=mcp-app" or "<html" not in content["text"]:
        raise ValueError("Packaged MCP interface resource is missing")
    if snapshot.get("isError") or not snapshot["structuredContent"]["sessions"]:
        raise ValueError("Packaged MCP cannot read the isolated fixture")
    if {field: snapshot["structuredContent"]["runtime"][field] for field in IDENTITY_FIELDS} != runtime:
        raise ValueError("Packaged MCP did not reuse the verified HTTP service and configuration")
    if four_columns:
        four_column_snapshot(snapshot["structuredContent"])
    remaining_time(deadline)
    return initialized["serverInfo"]


def read_http(port, path):
    with closing(HTTPConnection("127.0.0.1", port, timeout=3)) as connection:
        connection.request("GET", path)
        with connection.getresponse() as response:
            if response.status != 200:
                raise ValueError(f"Packaged HTTP endpoint unavailable: {path}")
            return json.load(response)


def verify(package, fixture, output, four_columns=False):
    package, fixture, output = package.resolve(), fixture.resolve(strict=True), output.resolve()
    if output.exists() or output.is_relative_to(ROOT) or output.is_relative_to(package):
        raise ValueError("Startup report must use a new path outside the source and package")
    manifest = verify_package(package)
    started = time.monotonic()
    report = {
        "schema_version": 1, "scope": "package_startup", "passed": False,
        "source_commit": manifest["source_commit"], "build_sha256": manifest["build_sha256"],
        "binary_sha256": manifest["binary_sha256"], "version": manifest["version"],
        "target": manifest["target"], "checks": [], "cleanup_confirmed": False,
        "started_at": datetime.now(timezone.utc).isoformat(),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix="csb-pkg-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            environment = child_environment(root)
            with socket.socket() as lease:
                lease.bind(("127.0.0.1", 0))
                port = lease.getsockname()[1]
            subprocess.run([str(fixture), "prepare", str(root), str(port)], cwd=ROOT,
                           env=environment, check=True, timeout=10, capture_output=True)
            if four_columns:
                config_path = root / "config.json"
                config = json.loads(config_path.read_text())
                config["four_columns"] = True
                del config["status_sections"]["unknown"]
                config_path.write_text(json.dumps(config))
            children = []
            try:
                children.append(Child(root, "fixture", [
                    str(fixture), "serve", str(root), str(package / "bin/run-plugin")], environment))
                record = {
                    "root": str(root), "url": f"http://127.0.0.1:{port}",
                    "program_sha256": manifest["binary_sha256"],
                }
                ready(children, record, threading.Event())
                runtime = {field: value for field, value in read_http(port, "/api/info").items()
                           if field in IDENTITY_FIELDS}
                if four_columns:
                    four_column_snapshot(read_http(port, "/api/sessions"))
                    report["four_columns"] = {"http": True}
                authority = json.loads((root / "fixture-authority.json").read_text())
                service = json.loads((root / "service-child.json").read_text())
                if (runtime["authority"] != authority or authority["pid"] != children[0].pid
                        or service["parent"] != authority["pid"] or runtime["pid"] != service["pid"]
                        or runtime["build"] != manifest["binary_sha256"]
                        or runtime["dataDir"] != str(root / "data")
                        or runtime["version"] != manifest["version"] or not runtime["configId"]):
                    raise ValueError("Packaged service backend or process identity differs")
                config_sha = digest(root / "config.json")
                report["runtime"] = runtime
                report["config_sha256"] = config_sha
                report["mcp"] = mcp(root, manifest["version"], runtime, four_columns)
                if four_columns:
                    report["four_columns"]["mcp"] = True
                report["mcp_child"] = json.loads((root / "mcp-child.json").read_text())
                if digest(root / "config.json") != config_sha:
                    raise ValueError("Isolated configuration changed during startup")
                if digest(package / "bin/codex-session-board") != manifest["binary_sha256"]:
                    raise ValueError("Package binary changed during startup")
                verify_package(package)
            finally:
                try:
                    reap(children)
                    report["cleanup_confirmed"] = True
                finally:
                    report["logs"] = {
                        name: (root / f"{name}.log").read_text()[-8000:]
                        for name in ("fixture", "service", "mcp") if (root / f"{name}.log").exists()
                    }
            report.update(passed=True, checks=CHECKS)
    except (OSError, ValueError, KeyError, RuntimeError, TypeError, subprocess.SubprocessError) as error:
        report["error"] = str(error)
        raise
    finally:
        report.update(seconds=time.monotonic() - started,
                      finished_at=datetime.now(timezone.utc).isoformat())
        output.write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--four-columns", action="store_true")
    arguments = parser.parse_args()
    print(json.dumps(verify(
        arguments.package, arguments.fixture, arguments.output, arguments.four_columns
    )))


if __name__ == "__main__":
    main()
