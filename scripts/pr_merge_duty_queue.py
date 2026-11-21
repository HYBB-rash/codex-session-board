import fcntl
import json
from pathlib import Path
import subprocess

from scripts import pr_queue, pr_queue_registry as registry
from scripts.pr_task_state import digest, git, now, save
from scripts.validate_candidate import identity


def locate(checker, claim, directory=None):
    repository, number = claim["repository"], claim["pr"]
    index = registry.registry(checker) / f"{digest([repository.lower(), number])}.json"
    if index.exists():
        return registry.locate(repository, number, checker)
    if claim["queue"]:
        path = Path(claim["queue"])
        if not (path / "queue.json").is_file():
            raise ValueError("The claimed PR's original queue is missing")
        return path
    if directory is not None:
        path = directory / f"queue-pr-{number}"
        if (path / "queue.json").is_file():
            return path
    return None


def read_queue(path, claim):
    state = json.loads((path / "queue.json").read_text())
    if (state["repository"] != claim["repository"] or state.get("feedback_mode") != "github"
            or state.get("rehearsal") or state.get("reference", "refs/heads/main") != "refs/heads/main"
            or [item["number"] for item in state["items"]] != [claim["pr"]]):
        raise ValueError("An existing queue owns this PR; inspect its scope before continuing")
    return state


def new_candidate(duty, checker, directory, claim, main):
    git(checker, "fetch", "--no-tags", "origin", "refs/heads/main")
    if git(checker, "rev-parse", "FETCH_HEAD") != main:
        raise ValueError("Remote main changed during candidate preparation; retry the round")
    branch = f"codex/integration-duty-pr-{claim['pr']}-{main[:12]}-{claim['head'][:12]}"
    path = directory / branch.rsplit("/", 1)[-1]
    if not path.exists():
        git(checker, "worktree", "add", "-b", branch, str(path), main)
    duty.verify_origin(path, claim["repository"])
    if git(path, "branch", "--show-current") != branch:
        raise ValueError("Candidate checkout belongs to a different branch")
    pr_queue.clean(path, main)
    return path


def fetch_head(candidate, claim):
    git(candidate, "fetch", "--no-tags", "origin", f"refs/pull/{claim['pr']}/head")
    if git(candidate, "rev-parse", "FETCH_HEAD") != claim["head"]:
        raise ValueError("PR head changed while fetching candidate input")


def replace_base(queue, state, candidate, main, item):
    with (queue / "queue.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if json.loads((queue / "queue.json").read_text()) != state:
            raise ValueError("Existing queue changed while preparing recombination")
        if state.get("batch_merged") or state["items"][0]["status"] in {"merged", "closed"}:
            raise ValueError("A terminal queue cannot be recombined")
        if state["items"][0].get("feedback"):
            raise ValueError("Resolve the existing feedback before recombining")
        previous = {key: state.get(key) for key in ("base", "project", "prefix", "integration", "frozen_candidate")}
        state.setdefault("recombinations", []).append({"at": now(), **previous})
        for key in ("integration", "frozen_candidate", "last_error", "handoff", "observation"):
            state.pop(key, None)
        state.update(project=str(candidate), base=main, prefix=main, merge_status="checking",
                     release_status="not_requested", batch_delivered=False)
        old_item = state["items"][0]
        old_item.update(item, status="submitted")
        for key in ("ingress", "recombine_required"):
            old_item.pop(key, None)
        save(queue / "queue.json", state)


def recover_combination(queue, state, candidate):
    item = state["items"][0]
    prefix, head = state["prefix"], git(candidate, "rev-parse", "HEAD")
    if item["status"] != "submitted" or item.get("feedback") or state.get("frozen_candidate"):
