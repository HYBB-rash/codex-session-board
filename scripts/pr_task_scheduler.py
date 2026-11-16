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
    if state["scheduler"] and state["scheduler"]["status"] == "installed":
        path = Path(state["scheduler"]["plist"])
        if not path.exists() or digest(plistlib.loads(path.read_bytes())) != state["scheduler"]["sha256"]:
            raise ValueError("Installed LaunchAgent ownership changed")
        if present(state):
            return
    launch_agents = Path.home() / "Library/LaunchAgents"
    launch_agents.mkdir(parents=True, exist_ok=True)
    label = f"com.codex-session-board.pr-task.{digest([state['repository'], state['branch']])[:20]}"
    path = launch_agents / f"{label}.plist"
    definition = {
        "Label": label,
        "ProgramArguments": [os.path.abspath(sys.executable), str(ROOT / "scripts/pr_task.py"),
                             "tick", "--state", state["directory"]],
        "WorkingDirectory": str(ROOT), "StartInterval": state["interval"], "RunAtLoad": True,
        "AbandonProcessGroup": True,
        "EnvironmentVariables": {
            key: value for key in ("HOME", "PATH", "CODEX_HOME", "GH_CONFIG_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME")
            if (value := os.environ.get(key)) is not None
        },
        "StandardOutPath": str(Path(state["directory"]) / "scheduler.log"),
        "StandardErrorPath": str(Path(state["directory"]) / "scheduler-error.log"),
    }
    configured = json.loads(os.environ.get("CODEX_GITHUB_COMMAND", '["gh"]'))
    if not isinstance(configured, list) or len(configured) != 1 or not isinstance(configured[0], str):
        raise ValueError("Background credentials need a single executable adapter, without inline secrets")
    executable = shutil.which(configured[0])
    if not executable:
        raise ValueError("GitHub CLI is missing from the background environment")
    definition["EnvironmentVariables"]["CODEX_GITHUB_COMMAND"] = json.dumps([executable])
    environment = definition["EnvironmentVariables"]
    observed = subprocess.run([executable, "api", "--hostname", "github.com",
                               f"repos/{state['repository']}", "--jq", ".full_name"],
                              env=environment, check=True, capture_output=True, text=True, timeout=30)
    if observed.stdout.strip() != state["repository"]:
        raise ValueError("Background GitHub authentication resolved a different repository")
    subprocess.run([sys.executable, str(ROOT / "scripts/pr_task.py"), "probe", "--state", state["directory"]],
                   env=environment, check=True, capture_output=True, timeout=120)
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
    state["outcome"] = {
        "result": "merged" if snapshot["state"] == "merged" else "closed_without_merge",
        "head": snapshot["head"], "merge_commit": snapshot["merge_commit"], "observed_at": now(),
    }
    persist(state)
    if not native.archive(state):
        return
    if state["scheduler"]:
        remove(state)
    state.update(phase="complete", cleanup_pending=False, completed_at=now())
    persist(state)
    record(state, "completed", outcome=state["outcome"])
