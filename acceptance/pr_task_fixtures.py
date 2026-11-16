import json
from pathlib import Path
import tempfile
import time
import uuid

from scripts.pr_task_state import digest, persist


HEAD = "a" * 40
BASE = "b" * 40


def task(test):
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    directory = Path(temporary.name).resolve()
    state = {
        "schema_version": 1, "id": str(uuid.uuid4()), "directory": str(directory),
        "project": str(directory), "repository": "owner/repo", "branch": "codex/task",
        "base": "main", "base_commit": BASE, "remote": "origin", "remote_url": "https://github.com/owner/repo",
        "route": {"thread_id": str(uuid.uuid4()), "expected_cwd": str(directory),
                  "repair_cwd": str(directory), "socket": str(directory / "rpc.sock"),
                  "backend": {"pid": 1, "started": 2}},
        "session_anchor": {"turn": "initial", "users": digest([])}, "archive": False,
        "pr": 7, "reviewers": ["owner"], "checks": ["changes"],
        "phase": "watching", "events": [], "completed": [], "active_event": None,
        "interval": 600, "max_rounds": 3, "rounds": 0, "deadline": time.time() + 3600,
        "next_poll_at": 0, "scheduler": None,
    }
    persist(state)
    snapshot = {"head": HEAD, "base": BASE, "url": "https://github.com/owner/repo/pull/7",
                "state": "open", "items": [], "ci": "pending", "merge_commit": None, "draft": False}
    return state, snapshot


def pull():
    return {"number": 7, "node_id": "PR_7", "html_url": "https://github.com/owner/repo/pull/7",
            "head": {"sha": HEAD, "ref": "codex/task", "repo": {"full_name": "owner/repo"}},
            "base": {"sha": BASE, "ref": "main", "repo": {"full_name": "owner/repo"}},
            "state": "open", "merged": False, "merge_commit_sha": None, "draft": False}


def review(identifier=1, status="CHANGES_REQUESTED"):
    return {"id": identifier, "state": status, "user": {"login": "owner"},
            "body": "Preserve the old data", "commit_id": HEAD, "html_url": "https://github.com/review"}


def feedback():
    return {"key": "feedback-key", "kind": "feedback", "id": "1", "body": "Fix the data loss",
            "head": HEAD, "url": "https://github.com/comment", "author": "owner"}


def saved(state):
    return json.loads((Path(state["directory"]) / "task.json").read_text())
