import argparse
from datetime import datetime, timezone
import fcntl
import json
from pathlib import Path

from scripts import pr_queue_handoff as handoff
from scripts import pr_queue_registry as registry


def execute(queue, state, directory, arguments):
    action = arguments.command
    if action == "resume":
        if arguments.item:
            queue.requeue(state, json.loads(arguments.item.read_text()), directory)
        return handoff.resume(state, directory, queue)
    queue.verify_checker(state)
    if (action in {"attempt", "finish", "requeue"} and state.get("feedback_mode") == "github"
            and not state.get("rehearsal")):
        handoff.refresh(state)
    if action == "notify":
        for item in state["items"]:
            if item.get("feedback"):
                queue.notify(state, item, directory)
    elif action == "requeue":
        queue.requeue(state, json.loads(arguments.item.read_text()), directory)
    elif action == "closeout":
        from scripts.pr_queue_closeout import closeout
        closeout(state)
    else:
        getattr(queue, action.replace("-", "_"))(state, directory)
    result = state.get("merge_status") or "ready"
    if result == "checking":
        result = "merged" if state.get("batch_merged") else "ready"
    for item in state["items"]:
        if item.get("feedback") and not item["feedback"].get("comment"):
            return handoff.describe(state, directory, "infrastructure-blocked", "Feedback has not been delivered")
        if item["status"] == "needs-fix":
            result = "needs-evidence" if item["feedback"]["kind"] == "delivery" else "needs-author"
    return handoff.describe(state, directory, result)


def run(queue, state, directory, arguments):
    registry.register(directory, state)
    record = handoff.begin(state, directory, arguments.command)
    if getattr(arguments, "started_at", None):
        record["request_started_at"] = arguments.started_at
    state["merge_status"] = "checking"
    try:
        result = execute(queue, state, directory, arguments)
        state.pop("last_error", None)
    except (Exception, KeyboardInterrupt) as error:
        code = state.get("merge_status", "checking")
        if code == "checking":
            code = "needs-user" if isinstance(error, ValueError) else "infrastructure-blocked"
        state["last_error"] = {"at": queue.now(), "type": type(error).__name__, "detail": str(error)}
        state.setdefault("errors", []).append(state["last_error"])
        result = handoff.describe(state, directory, code, str(error))
    handoff.end(state, directory, record, result)
    return state


def main(queue):
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("init", "attempt", "notify", "requeue", "finish",
                                          "retry-candidate", "closeout", "status", "resume"))
    parser.add_argument("--state", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--pr", type=int)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--item", type=Path)
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--started-at")
    arguments = parser.parse_args()
    if arguments.started_at:
        started = datetime.fromisoformat(arguments.started_at)
        if started.tzinfo is None or started > datetime.now(timezone.utc):
            parser.error("--started-at requires a timezone-aware past instant")
    if not arguments.state and (not arguments.repository or not arguments.pr or arguments.pr < 1):
        parser.error("Use --state or --repository OWNER/REPO --pr NUMBER")
    if arguments.command == "init" and (not arguments.state or not arguments.manifest):
        parser.error("init requires --state and --manifest")
    if (arguments.refresh or arguments.release) and arguments.command != "status":
        parser.error("--refresh and --release are read-only status options")
    try:
        directory = arguments.state.resolve() if arguments.state else registry.locate(
            arguments.repository, arguments.pr)
        if arguments.command == "init":
            state = queue.initialize(arguments.manifest, directory)
            registry.register(directory, state)
        elif arguments.command == "status":
            state = json.loads((directory / "queue.json").read_text())
            if arguments.refresh:
                state["observation"] = handoff.observe(state)
                state["handoff"] = handoff.describe(state, directory, state["observation"]["result"],
                                                    state["observation"]["detail"])
                state["handoff"].update(read_only=True, observed_at=state["observation"]["observed_at"])
            if arguments.release:
                queue.refresh_release(state)
        else:
            with (directory / "queue.lock").open("a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                state = json.loads((directory / "queue.json").read_text())
                if arguments.repository and state["repository"].lower() != arguments.repository.lower():
                    raise ValueError("Selected queue belongs to another repository")
                if arguments.pr and not any(item["number"] == arguments.pr for item in state["items"]):
                    raise ValueError("Selected queue does not contain the requested PR")
