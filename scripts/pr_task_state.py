from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()


def save(path, value):
    descriptor, pending = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as stream:
            json.dump(value, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(pending, path)
    finally:
        if os.path.exists(pending):
            os.unlink(pending)


def git(project, *arguments):
    return subprocess.check_output(["git", *arguments], cwd=project, text=True,
                                   stderr=subprocess.PIPE, timeout=60).strip()


def private_directory(directory):
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    metadata = directory.stat()
    if metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError("Task records require a private directory owned by this user")
    return directory


@contextmanager
def locked(directory, wait=False):
    directory = private_directory(directory)
    with (directory / "task.lock").open("a") as stream:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
        yield directory


def load(directory):
    state = json.loads((directory / "task.json").read_text())
    if state["schema_version"] != 1 or state["directory"] != str(directory):
        raise ValueError("Task identity or record location changed")
    return state


def persist(state):
    state["updated_at"] = now()
    save(Path(state["directory"]) / "task.json", state)


def record(state, kind, **details):
    entry = {"at": now(), "kind": kind, **details}
    with (Path(state["directory"]) / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(entry, ensure_ascii=False) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def checkout(state, clean=False):
    project = Path(state["project"]).resolve(strict=True)
    if git(project, "rev-parse", "--show-toplevel") != str(project):
        raise ValueError("Task directory is no longer its registered worktree")
    if git(project, "branch", "--show-current") != state["branch"]:
        raise ValueError("Task branch changed")
    if git(project, "remote", "get-url", state["remote"]) != state["remote_url"]:
        raise ValueError("Task remote changed")
    if clean and git(project, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Commit the task changes before publishing")
    return git(project, "rev-parse", "HEAD")


def summary(state):
    result = {key: state.get(key) for key in (
        "id", "repository", "pr", "phase", "directory", "project", "branch",
        "last_snapshot", "active_event", "last_error", "next_poll_at", "outcome",
        "scheduler", "updated_at",
    )}
    if result["last_snapshot"]:
        snapshot = result["last_snapshot"]
        result["last_snapshot"] = {key: snapshot[key] for key in ("head", "state", "ci", "url")}
        result["last_snapshot"]["feedback_count"] = len(snapshot["items"])
    return result


def defer(state, error, seconds, pause=False):
    state["last_error"] = {"at": now(), "type": type(error).__name__, "detail": str(error)}
    state["next_poll_at"] = time.time() + seconds
    if pause:
        state["phase"] = "paused"
    record(state, "error", **state["last_error"])
    persist(state)
