import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import tarfile
import tempfile
import threading
import time
from unittest.mock import patch


@contextmanager
def foreign_peer(root):
    path = root / "foreign.sock"
    events, errors = [], []
    stopped = threading.Event()
    with socket.socket(socket.AF_UNIX) as server:
        server.bind(str(path))
        os.chmod(path, 0o600)
        server.listen()
        server.settimeout(0.1)

        def accept():
            while not stopped.is_set():
                try:
                    stream, _ = server.accept()
                except TimeoutError:
                    continue
                try:
                    with stream:
                        stream.settimeout(1)
                        events.append(len(stream.recv(1)))
                except OSError as error:
                    errors.append(str(error))

        worker = threading.Thread(target=accept)
        worker.start()
        try:
            yield path, events
        finally:
            stopped.set()
            worker.join(timeout=2)
            if worker.is_alive() or errors:
                raise RuntimeError(f"Foreign peer did not finish cleanly: {errors}")


def wrapper(root, fixture, modification):
    path = root / "fixture-wrapper"
    path.write_text(
        f"#!{sys.executable}\nimport json,os,pathlib,subprocess,sys\n"
        f"fixture={str(fixture)!r}\n"
        "if sys.argv[1]=='prepare':\n"
        " subprocess.run([fixture,*sys.argv[1:]],check=True)\n"
        " path=pathlib.Path(sys.argv[2])/'config.json';settings=json.loads(path.read_text())\n"
        f" {modification}\n"
        " path.write_text(json.dumps(settings))\n"
        "else:os.execv(fixture,[fixture,*sys.argv[1:]])\n"
    )
    path.chmod(0o700)
    return path


def verify_case(startup, package, fixture, output, name, expected=None, drift=None, exit_code=None):
    saved = {}
    original_ready, original_reap = startup.ready, startup.reap

    def ready(*arguments):
        original_ready(*arguments)
        if drift:
            path = Path(arguments[1]["root"]) / "config.json"
            settings = json.loads(path.read_text())
            settings["poll_seconds"] = 0 if drift == "invalid" else settings["poll_seconds"] + 1
            path.write_text(json.dumps(settings))

    def reap(children):
        original_reap(children)
        if children:
            saved.update({path.name: json.loads(path.read_text())
                          for path in children[0].state.parent.glob("*-child.json")})

    error = None
    with patch.object(startup, "ready", ready), patch.object(startup, "reap", reap):
        try:
            startup.verify(package, fixture, output / f"{name}.json")
        except (Exception, KeyboardInterrupt) as failure:
            error = failure
    report = json.loads((output / f"{name}.json").read_text())
    (output / f"{name}-processes.json").write_text(json.dumps(saved, indent=2) + "\n")
    if report.get("cleanup_confirmed") is not True:
        raise ValueError(f"{name}: process cleanup was not confirmed")
    if expected is None:
        if error or report.get("passed") is not True:
            raise RuntimeError(f"{name}: actual package startup failed: {error}")
    elif error is None or report.get("passed") is not False or expected not in report.get("error", ""):
        raise ValueError(f"{name}: the expected failure was not detected: {report.get('error')}")
    if exit_code is not None and saved.get("mcp-child.json", {}).get("exit") != exit_code:
        raise ValueError(f"{name}: actual MCP exit differs")
    return {"name": name, "passed": True, "expected_rejection": expected is not None,
            "actual_startup_passed": report["passed"], "cleanup_confirmed": True,
            "report": str(output / f"{name}.json"), "seconds": report["seconds"]}


def verify(project, built, fixture, output, connection=False):
    project, built = project.resolve(strict=True), built.resolve(strict=True)
    fixture, output = fixture.resolve(strict=True), output.resolve()
    if output.is_relative_to(project) or output.is_relative_to(built):
        raise ValueError("Package scenario evidence must remain outside the source and package")
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    record = {"scope": "pr_package_scenarios", "passed": False, "cases": [],
              "started_at": datetime.now(timezone.utc).isoformat(),
              "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
              "verifier_sha256": hashlib.sha256(
                  (project / "scripts/verify_package_startup.py").read_bytes()).hexdigest()}
    sys.path.insert(0, str(project))
    sys.path.insert(0, str(project / "scripts"))
    import verify_package_startup as startup
    from release_plugin import unpack
    try:
        archive = output / "plugin.tar.gz"
        with tarfile.open(archive, "w:gz") as contents:
            for path in sorted(built.iterdir()):
                contents.add(path, arcname=path.name)
        package = output / "unpacked"
        record["manifest"] = unpack(archive, package)
        record["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
        record["cases"].append(verify_case(startup, package, fixture, output, "success"))
        record["cases"].append(verify_case(
            startup, package, fixture, output, "mcp-config-drift", "cannot read", drift=True, exit_code=0))
        record["cases"].append(verify_case(
            startup, package, fixture, output, "mcp-failure-exit",
            "unsuccessfully", drift="invalid", exit_code=1))
        with tempfile.TemporaryDirectory(prefix="csb-pr-fault-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            with foreign_peer(root) as (path, events):
                wrong = wrapper(root, fixture, f"settings['desktop_socket']={str(path)!r}")
                record["cases"].append(verify_case(
                    startup, package, wrong, output, "wrong-backend", "不是启动本看板"))
                if not events or any(events):
                    raise ValueError(f"Wrong backend received RPC bytes or was not contacted: {events}")
                record["wrong_backend_bytes"] = list(events)
            invalid = wrapper(root, fixture, "settings['poll_seconds']=0")
            record["cases"].append(verify_case(
                startup, package, invalid, output, "service-failure-exit", "提前退出"))
        if connection:
            from pr_connection_probe import verify_sidebar
            record["cases"].append(verify_sidebar(startup, package, fixture, output))
        record["passed"] = True
        return 0
    except (Exception, KeyboardInterrupt) as error:
        record["error"] = {"type": type(error).__name__, "detail": str(error)}
        print(str(error), file=sys.stderr)
        return 1
    finally:
        record.update(seconds=time.monotonic() - started,
                      finished_at=datetime.now(timezone.utc).isoformat())
        (output / "SCENARIOS.json").write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps({"passed": record["passed"], "cases": len(record["cases"]),
                          "seconds": record["seconds"]}), flush=True)


def main():
    parser = argparse.ArgumentParser()
    for name in ("project", "package", "fixture", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--connection", action="store_true")
    arguments = parser.parse_args()
    return verify(arguments.project, arguments.package, arguments.fixture,
                  arguments.output, arguments.connection)


if __name__ == "__main__":
    sys.exit(main())
