import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch

import websocket

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.dev_environment.configuration import child_environment
from scripts.dev_environment.processes import Child, reap
from plugin_package import hashes
from update_plugin import cache_directory, cached_package, command_error, market_registration, target, update
import update_plugin
from board_update_job import REPOSITORY
from reload_plugins import verify_loaded


SESSION_ID = "upgrade-verification"
NOTE = "验证升级后保留备注"


class UpgradeReport:
    def __init__(self, path, arguments):
        self.path = path
        self.started = time.monotonic()
        self.record = {
            "schema_version": 1, "scope": "isolated_native_upgrade",
            "started_at": datetime.now(timezone.utc).isoformat(),
            "source": {"repository": arguments.repository, "candidate": f"refs/heads/candidate/{target()}",
                       "transport": "local_fixture" if arguments.fixture else "github",
                       "fixture": str(arguments.fixture) if arguments.fixture else None},
            "requested_previous_revision": arguments.previous,
            "trigger": "verification_command",
            "steps": [],
            "target_host_acceptance": {
                "status": "pending", "missing": ["installed_board_button", "daily_host_reload",
                                                  "daily_board_and_notes_readback"]},
            "passed": False,
        }

    def step(self, label, action):
        started = time.monotonic()
        step = {"name": label}
        self.record["steps"].append(step)
        try:
            result = action()
            step.update(status="passed", evidence=result)
            return result
        except Exception as error:
            step.update(status="failed", error=command_error(error))
            raise
        finally:
            step["seconds"] = round(time.monotonic() - started, 3)

    def finish(self):
        self.record["seconds"] = round(time.monotonic() - self.started, 3)
        self.path.write_text(json.dumps(self.record, ensure_ascii=False, indent=2) + "\n")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identity(receipt, stages):
    release = receipt["release"]
    return {"version": release["version"], "source_commit": release["source_commit"],
            "binary_sha256": release["binary_sha256"], "marketplace_revision": receipt.get("revision"),
            "phase": receipt["phase"], "worker_stages": stages,
            "reload_error": receipt.get("reload_error")}


@contextmanager
def app_server(root, codex, environment, label):
    address = Path(environment["CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET"])
    address.unlink(missing_ok=True)
    server = Child(root, label, [codex, "app-server", "--listen", f"unix://{address}"], environment)
    try:
        deadline = time.monotonic() + 10
        while not address.is_socket():
            if time.monotonic() >= deadline or server.poll() is not None:
                raise RuntimeError("Isolated Codex app-server did not open its socket")
            time.sleep(0.05)
        with socket.socket(socket.AF_UNIX) as transport:
            transport.settimeout(20)
            transport.connect(str(address))
            connection = websocket.create_connection("ws://localhost/rpc", socket=transport,
                                                     timeout=20, suppress_origin=True)
            try:
                serial = 0

                def request(method, params):
                    nonlocal serial
                    serial += 1
                    connection.send(json.dumps({"id": serial, "method": method, "params": params}))
                    deadline = time.monotonic() + 20
                    while time.monotonic() < deadline:
                        connection.settimeout(max(0.01, deadline - time.monotonic()))
                        result = json.loads(connection.recv())
                        if result.get("id") == serial and not result.get("method"):
                            if "error" in result:
                                raise RuntimeError(f"Isolated Codex rejected {method}: {result['error']}")
                            return result["result"]
                    raise TimeoutError(f"Isolated Codex did not answer {method}")

                request("initialize", {"clientInfo": {"name": "board_upgrade_verification", "version": "1"},
                                       "capabilities": {"experimentalApi": True}})
                connection.send(json.dumps({"method": "initialized"}))
                yield request
            finally:
                connection.close()
    finally:
        reap([server])


def read_board(request, root, version, expect_note=False):
    thread = request("thread/start", {"cwd": str(root), "ephemeral": True})["thread"]["id"]
    note_preserved = False

    def read(identifier, method, params):
        nonlocal note_preserved
        response = request(method, params)
        if method == "mcpServer/tool/call":
            if response.get("isError"):
                raise RuntimeError(f"Isolated board_snapshot failed: {response.get('content')}")
            rows = response.get("structuredContent", {}).get("sessions", [])
            note_preserved = any(row["id"] == SESSION_ID and row["state"] == "blocked"
                                 and row["note"] == NOTE and row["version"] == 7 for row in rows)
        return response

    server = verify_loaded(read, version, thread)
    if expect_note:
        require(note_preserved, "Isolated board lost its session state or note")
    return thread, {"version": server["serverInfo"]["version"], "board_snapshot": "passed",
                    "runtime_status": server["runtimeStatus"], "fixture_note_confirmed": note_preserved}


def verify_orphan_recovery(codex, root, state, environment, previous, install):
    update_plugin.remove_market(codex)
    require(market_registration(codex) is None, "Expected an unregistered old cache")
    old_cache = cache_directory(previous["version"])
    old_files = hashes(old_cache)
    old_receipt = (state / "current.json").read_bytes()
    observed = {}

    def fail_after_install(*args, home=None):
        observed["version"] = cached_package(*args, home=home)["version"]
        observed["old_cache_pruned"] = not cache_directory(previous["version"], home=home).exists()
        (cache_directory(observed["version"], home=home) / "RELEASE.json").write_text("null\n")
        return cached_package(*args, home=home)

    with patch.object(update_plugin, "cached_package", fail_after_install):
        try:
            install(running=previous["binary_sha256"])
        except RuntimeError as error:
            require(str(error.__cause__) == "Plugin release manifest must be an object"
                    and "已恢复原缓存" in str(error), str(error))
        else:
            raise ValueError("Expected a reported native install failure")
    require(observed.get("old_cache_pruned") and observed["version"] != previous["version"],
            "The candidate did not replace the old cache")
    require(market_registration(codex) is None and
            hashes(old_cache) == old_files and (state / "current.json").read_bytes() == old_receipt,
            "Old cache, registration or successful receipt changed")
    with app_server(root, codex, environment, "orphan-restart") as request:
        _, board = read_board(request, root, previous["version"])
    return {"injected": "null_installed_manifest", "installed_before_failure": observed["version"],
            "restored_binary_sha256": previous["binary_sha256"], "old_board_reopened": board}


def verify(arguments, report, root):
    codex = shutil.which(arguments.codex)
    require(codex, "Codex CLI is required")
    environment = child_environment(root)
    environment.update(PATH=os.environ["PATH"], GIT_TERMINAL_PROMPT="0", GIT_CONFIG_NOSYSTEM="1",
                       CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET=str(root / "app.sock"))
    environment.pop("CODEX_SESSION_BOARD_CONFIG")
    config = Path(environment["XDG_CONFIG_HOME"]) / "codex-session-board"
    config.mkdir()
    (config / "config.json").write_text(json.dumps({
        "data_dir": str(root / "board"), "sync_titles": False, "codex_command": [codex, "app-server", "--stdio"],
    }))
    if arguments.fixture:
        fixture = arguments.fixture.resolve(strict=True).as_uri()
        environment.update(GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0=f"url.{fixture}.insteadOf",
                           GIT_CONFIG_VALUE_0=arguments.repository)
    state = Path(environment["XDG_DATA_HOME"]) / "codex-session-board/installation"

    def install(revision=None, running=None, reload=False):
        stages = []
        report.record["steps"][-1]["observed_phases"] = stages
        receipt = update(arguments.repository, "candidate", revision, state, codex, reload=reload,
                         running_build=running, progress=lambda phase, **fields: stages.append(phase))
        return identity(receipt, stages)

    with patch.dict(os.environ, environment, clear=True):
        previous = report.step("install_previous_native_package", lambda: install(arguments.previous))
        if arguments.exercise_recovery:
            home = Path(environment["CODEX_HOME"])
            config_path = home / "config.toml"
            instructions = 'model_instructions_file = "instructions.md"\n'
            (home / "instructions.md").write_text("Isolated upgrade verification\n")
            config_path.write_text(instructions + config_path.read_text())
            report.step("native_add_with_relative_config", lambda: update_plugin.add_plugin(codex))
            report.step("unregistered_cache_recovery_and_host_restart",
                        lambda: verify_orphan_recovery(codex, root, state, environment, previous, install))
            report.step("register_baseline_for_remaining_checks", lambda: install(arguments.previous))
        with app_server(root, codex, environment, "before") as request:
            report.step("read_previous_from_host", lambda: read_board(request, root, previous["version"])[1])
            with sqlite3.connect(root / "board/status.sqlite3") as database:
                database.execute(
                    "insert into sessions(id,title,cwd,updated_at,seen_at,selected_state,note,version) "
                    "values (?, 'Upgrade verification', ?, 1, 1, 'blocked', ?, 7)",
                    (SESSION_ID, str(root), NOTE))
            thread, _ = report.step("read_baseline_state_and_note",
                                    lambda: read_board(request, root, previous["version"], expect_note=True))
            os.environ["CODEX_THREAD_ID"] = thread
            if arguments.exercise_recovery:
                run_native = update_plugin.run
                fail_next = True

                def fail_once(command, **kwargs):
                    nonlocal fail_next
                    if fail_next and command[:4] == [codex, "plugin", "marketplace", "add"]:
                        fail_next = False
                        raise subprocess.TimeoutExpired(command, 120)
                    return run_native(command, **kwargs)

                def verify_recovery():
                    with patch.object(update_plugin, "run", fail_once):
                        try:
                            install(running=previous["binary_sha256"])
                        except RuntimeError as error:
                            require("已恢复上一个插件市场及缓存" in str(error), str(error))
                        else:
                            raise ValueError("Expected a reported native install failure")
                    require(not fail_next, "Injected fault did not run")
                    restored = cached_package(codex, previous["version"], previous["marketplace_revision"])
                    require(restored["binary_sha256"] == previous["binary_sha256"], "Old cache was not restored")
                    _, board = read_board(request, root, previous["version"], expect_note=True)
                    return {"injected": "marketplace_add_timeout", "old_board_reopened": board,
                            "restored_marketplace_revision": previous["marketplace_revision"]}

                report.step("native_failure_and_previous_board_recovery", verify_recovery)
            candidate = report.step("fetch_verify_and_native_install_candidate",
                                    lambda: install(running=previous["binary_sha256"], reload=True))
            require(candidate["version"] != previous["version"], "Candidate did not advance to a new version")
            require(candidate["marketplace_revision"], "Candidate installation has no immutable revision")
            if arguments.exercise_recovery:
                require(update_plugin.read_config(config_path)["model_instructions_file"] == "instructions.md",
                        "Upgrade changed the relative instructions setting")
            if candidate["phase"] == "desktop_loaded":
                report.step("read_new_board_from_reloaded_host",
                            lambda: read_board(request, root, candidate["version"], expect_note=True)[1])
            else:
                report.record["steps"].append({"name": "automatic_host_reload", "status": "pending",
                                               "evidence": candidate["reload_error"] or candidate["phase"]})
        os.environ.pop("CODEX_THREAD_ID", None)
        with app_server(root, codex, environment, "after") as request:
            report.step("independent_host_and_board_readback",
                        lambda: read_board(request, root, candidate["version"], expect_note=True)[1])

        def verify_latest():
            registration = market_registration(codex)
            receipt = (state / "current.json").stat().st_mtime_ns
            package = cache_directory(candidate["version"]).stat().st_ino
            latest = install(running=candidate["binary_sha256"])
            require(latest["phase"] == "up_to_date" and market_registration(codex) == registration
                    and (state / "current.json").stat().st_mtime_ns == receipt
                    and cache_directory(candidate["version"]).stat().st_ino == package,
                    "Already-current candidate changed the native installation")
            return latest

        report.step("already_current_without_reinstall", verify_latest)


def main():
    parser = argparse.ArgumentParser(description="Verify a candidate update in a disposable native Codex home.")
    parser.add_argument("--previous", required=True, help="Immutable marketplace revision for the installed baseline")
    parser.add_argument("--output", required=True, type=Path, help="New JSON report path")
    parser.add_argument("--fixture", type=Path, help="Optional local Git repository with baseline and candidate revisions")
    parser.add_argument("--repository", default=REPOSITORY, help="Git repository URL to exercise")
    parser.add_argument("--codex", default="codex")
    parser.add_argument("--exercise-recovery", action="store_true")
    arguments = parser.parse_args()
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.touch(mode=0o600, exist_ok=False)
    report = UpgradeReport(arguments.output, arguments)
    try:
        with tempfile.TemporaryDirectory(prefix="csb-upgrade-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            try:
                verify(arguments, report, root)
            except Exception:
                report.record["host_logs"] = {
                    log.name: log.read_text(errors="replace")[-6000:] for log in root.glob("*.log")
                }
                raise
        report.record["passed"] = True
    except Exception as error:
        report.record["error"] = command_error(error)
        raise SystemExit(command_error(error)) from error
    finally:
        report.finish()


if __name__ == "__main__":
    main()
