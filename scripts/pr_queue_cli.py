import argparse
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
        result = "ready"
    for item in state["items"]:
        if item.get("feedback") and not item["feedback"].get("comment"):
            return handoff.describe(state, directory, "infrastructure-blocked", "Feedback has not been delivered")
        if item["status"] == "needs-fix":
            result = "needs-evidence" if item["feedback"]["kind"] == "delivery" else "needs-author"
    return handoff.describe(state, directory, result)


def run(queue, state, directory, arguments):
    registry.register(directory, state)
    state.setdefault("checker_project", str(queue.ROOT))
    record = handoff.begin(state, directory, arguments.command)
    state["merge_status"] = "checking"
    try:
        result = execute(queue, state, directory, arguments)
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
    arguments = parser.parse_args()
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
                run(queue, state, directory, arguments)
        print(json.dumps(state, ensure_ascii=False, indent=2))
        result = state.get("handoff", {}).get("result")
        if arguments.command == "closeout" and not state["closeout"]["complete"]:
            return 2
        return 2 if arguments.command != "status" and result not in {None, "ready", "merged", "waiting-ci"} else 0
    except BlockingIOError:
        print("Queue is busy; retry this same invocation")
        return 75
    except Exception as error:
        print(str(error))
        return 2
