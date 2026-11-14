import argparse
from datetime import datetime, timezone
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


def mcp(package, root, environment, version):
    requests = [
        {"id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "clientInfo": {"name": "package-startup", "version": "1"},
        }},
        {"method": "notifications/initialized"},
        {"id": 2, "method": "resources/read", "params": {"uri": "ui://session-board/index.html"}},
        {"id": 3, "method": "tools/call", "params": {"name": "board_snapshot", "arguments": {}}},
    ]
    result = subprocess.run(
        [str(package / "bin/run-plugin"), "--config", str(root / "config.json"), "mcp"],
        input="".join(json.dumps({"jsonrpc": "2.0", **request}) + "\n" for request in requests),
        cwd=root, env=environment, text=True, capture_output=True, timeout=30, check=True,
    )
    replies = [json.loads(line) for line in result.stdout.splitlines()]
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
    return initialized["serverInfo"]


def verify(package, fixture, output):
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
            children = []
            try:
                commands = [
                    [str(fixture), "serve", str(root)],
                    [str(package / "bin/run-plugin"), "--config", str(root / "config.json"), "serve"],
                ]
                for name, command in zip(("fixture", "service"), commands):
                    children.append(Child(root, name, command, environment))
                record = {
                    "root": str(root), "url": f"http://127.0.0.1:{port}",
                    "program_sha256": manifest["binary_sha256"],
                }
                ready(children, record, threading.Event())
                report["mcp"] = mcp(package, root, environment, manifest["version"])
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
                        for name in ("fixture", "service") if (root / f"{name}.log").exists()
                    }
            report.update(passed=True, checks=CHECKS)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
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
    arguments = parser.parse_args()
    print(json.dumps(verify(arguments.package, arguments.fixture, arguments.output)))


if __name__ == "__main__":
    main()
