import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import time

from scripts import pr_task_github as forge
from scripts import pr_task_native as native
from scripts.pr_task_state import ROOT, digest, now, persist, record
from scripts.shared_desktop import process_identity


def command(*arguments, check=True):
    return subprocess.run(["/bin/launchctl", *arguments], text=True, capture_output=True,
                          timeout=20, check=check)


def target(state):
    return f"gui/{os.getuid()}/{state['scheduler']['label']}"


def present(state):
    result = command("print", target(state), check=False)
    if result.returncode == 0:
        return True
    if result.returncode == 113:
        return False
    raise RuntimeError(f"launchctl cannot confirm service state: {result.returncode}: {result.stderr.strip()}")


def install(state):
    if sys.platform != "darwin":
        raise ValueError("Built-in scheduling currently supports macOS; invoke tick with an external scheduler")
    if state["phase"] in {"complete", "finalizing"}:
        raise ValueError("Cannot schedule an ended task")
    if not state["pr"]:
        raise ValueError("Publish or attach a PR before enabling its watcher")
    launch_agents = Path.home() / "Library/LaunchAgents"
    launch_agents.mkdir(parents=True, exist_ok=True)
    label = f"com.codex-session-board.pr-task.{digest([state['repository'], state['branch']])[:20]}"
    path = launch_agents / f"{label}.plist"
    definition = {
        "Label": label,
        "ProgramArguments": [str(Path(sys.executable).resolve()), str(ROOT / "scripts/pr_task.py"),
                             "tick", "--state", state["directory"]],
        "WorkingDirectory": str(ROOT), "StartInterval": state["interval"], "RunAtLoad": True,
        "AbandonProcessGroup": True,
        "EnvironmentVariables": {
            key: value for key in ("HOME", "PATH", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME")
            if (value := os.environ.get(key)) is not None
        },
        "StandardOutPath": str(Path(state["directory"]) / "scheduler.log"),
        "StandardErrorPath": str(Path(state["directory"]) / "scheduler-error.log"),
    }
    executable = shutil.which("gh")
    if not executable:
        raise ValueError("GitHub CLI is missing from the background environment")
    definition["EnvironmentVariables"]["CODEX_GITHUB_COMMAND"] = json.dumps([executable])
    environment = definition["EnvironmentVariables"]
    subprocess.run([executable, "api", f"repos/{state['repository']}", "--jq", ".full_name"],
                   env=environment, check=True, capture_output=True, timeout=30)
    encoded = plistlib.dumps(definition)
    if path.exists() and path.read_bytes() != encoded:
        raise ValueError("An existing LaunchAgent differs; do not overwrite it")
    state["scheduler"] = {"label": label, "plist": str(path), "sha256": digest(definition),
                          "status": "installing"}
    persist(state)
    if not path.exists():
        with path.open("xb") as stream:
            os.chmod(path, 0o600)
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
    if not present(state):
        command("bootstrap", f"gui/{os.getuid()}", str(path))
    if not present(state):
        raise RuntimeError("LaunchAgent installation did not become observable")
    state["scheduler"]["status"] = "installed"
    persist(state)
    record(state, "scheduled", label=label, interval=state["interval"])


def remove(state):
    scheduler = state["scheduler"]
    path = Path(scheduler["plist"])
    if path.exists():
        if digest(plistlib.loads(path.read_bytes())) != scheduler["sha256"]:
            raise ValueError("LaunchAgent ownership changed; refusing to remove it")
        path.unlink()
    if present(state):
        result = command("bootout", target(state), check=False)
        if present(state):
            raise RuntimeError(f"LaunchAgent removal is unconfirmed: {result.returncode}: {result.stderr.strip()}")
    scheduler["status"] = "removed"
    scheduler["removed_at"] = now()
    persist(state)


def finalizer_running(state):
    identity = state.get("finalizer")
    if not identity:
        return False
    try:
        return process_identity(identity["pid"]) == identity
    except (OSError, RuntimeError, subprocess.SubprocessError):
        return False


def finish(state):
    if not state["scheduler"]:
        state.update(phase="complete", completed_at=now())
        persist(state)
        record(state, "completed", outcome=state["outcome"])
        return
    if finalizer_running(state):
        return
    state["cleanup_pending"] = True
    persist(state)
    directory = Path(state["directory"])
    with (directory / "finalizer.log").open("a") as log:
        child = subprocess.Popen([sys.executable, str(ROOT / "scripts/pr_task.py"), "finalize",
                                  "--state", str(directory)], cwd=ROOT, start_new_session=True,
                                 close_fds=True, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    state["finalizer"] = process_identity(child.pid)
    persist(state)


def finalize(state):
    if state["phase"] == "complete":
        return
    if state["phase"] != "finalizing":
        raise ValueError("Only terminal tasks may finalize")
    snapshot = forge.snapshot(state)
    if snapshot["state"] not in {"merged", "closed"}:
        state["phase"] = "needs-human"
        persist(state)
        raise ValueError("PR reopened during cleanup; inspection is required")
    if not native.archive(state):
        return
    remove(state)
    state.update(phase="complete", cleanup_pending=False, completed_at=now())
    persist(state)
    record(state, "completed", outcome=state["outcome"])


def reconcile_cleanup(state):
    if (state["phase"] == "finalizing" and state.get("cleanup_pending")
            and not finalizer_running(state) and not Path(state["scheduler"]["plist"]).exists()
            and not present(state)):
        finalize(state)
