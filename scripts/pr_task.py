import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
if sys.version_info < (3, 11):
    from scripts.validation_environment import choose_python
    selected_python = choose_python(Path(__file__).resolve().parents[1], dict(os.environ))["path"]
    os.execv(selected_python, [selected_python, __file__, *sys.argv[1:]])

from scripts import pr_task_github as forge
from scripts import pr_task_init as setup
from scripts import pr_task_lifecycle as lifecycle
from scripts import pr_task_publish as publish
from scripts import pr_task_scheduler as scheduler
from scripts.pr_task_state import checkout, defer, load, locked, persist, record, summary


def parser():
    result = argparse.ArgumentParser(description="项目内 PR 作者任务：验证发布、无模型巡检、原会话修复和收尾")
    commands = result.add_subparsers(dest="command", required=True)
    initial = commands.add_parser("init")
    initial.add_argument("--state", required=True, type=Path)
    initial.add_argument("--project", type=Path, default=Path.cwd())
    initial.add_argument("--repository", required=True)
    initial.add_argument("--thread", default=os.environ.get("CODEX_THREAD_ID"))
    initial.add_argument("--socket", required=True, type=Path)
    initial.add_argument("--base", default="main")
    initial.add_argument("--remote", default="origin")
    initial.add_argument("--reviewer", action="append", default=[])
    initial.add_argument("--check", action="append", default=[])
    initial.add_argument("--interval", type=int, default=600)
    initial.add_argument("--max-rounds", type=int, default=3)
    initial.add_argument("--max-hours", type=int, default=72)
    initial.add_argument("--archive", action="store_true")
    for name in ("status", "tick", "schedule", "finalize", "probe", "pause", "resume", "attach", "submit",
                 "claim", "complete", "rebind"):
        command = commands.add_parser(name)
        command.add_argument("--state", required=True, type=Path)
        if name in ("claim", "complete"):
            command.add_argument("--event", required=True)
        if name == "complete":
            command.add_argument("--result", choices=("fixed", "blocked"), required=True)
            command.add_argument("--message-file", required=True, type=Path)
        if name in ("attach", "submit"):
            command.add_argument("--no-schedule", action="store_true")
        if name == "attach":
            command.add_argument("--pr", required=True, type=int)
        if name == "submit":
            command.add_argument("--title")
            command.add_argument("--body-file", type=Path)
        if name == "tick":
            command.add_argument("--force", action="store_true")
        if name == "resume":
            command.add_argument("--additional-rounds", type=int, default=0)
            command.add_argument("--extend-hours", type=int, default=0)
            command.add_argument("--retry-failed", action="store_true")
        if name == "rebind":
            command.add_argument("--socket", required=True, type=Path)
            command.add_argument("--accept-input", action="store_true")
    feedback = commands.add_parser("feedback")
    feedback.add_argument("--repository", required=True)
    feedback.add_argument("--pr", type=int, required=True)
    feedback.add_argument("--head", required=True)
    feedback.add_argument("--request-id", required=True)
    feedback.add_argument("--body-file", required=True, type=Path)
    return result


def execute(state, arguments):
    action = arguments.command
    if action == "status":
        return summary(state)
    if action == "tick":
        lifecycle.tick(state, arguments.force)
    elif action == "claim":
        return lifecycle.claim(state, arguments.event)
    elif action == "complete":
        lifecycle.complete(state, arguments.event, arguments.result, arguments.message_file.read_text())
    elif action == "submit":
        publish.submit(state, arguments.title, arguments.body_file.read_text() if arguments.body_file else None)
        if state["phase"] == "finalizing":
            lifecycle.tick(state, force=True)
        if not arguments.no_schedule and state["phase"] != "complete":
            scheduler.install(state)
    elif action == "attach":
        if state["phase"] in {"complete", "finalizing"}:
            raise ValueError("Ended task records cannot attach a PR again")
        if state["pr"] not in (None, arguments.pr) or arguments.pr <= 0:
            raise ValueError("This task cannot switch to another PR")
        previous = state["pr"]
        state["pr"] = arguments.pr
        try:
            snapshot = forge.snapshot(state)
            if snapshot["state"] != "open":
                raise ValueError("Attach only an open PR")
            if checkout(state, clean=True) != snapshot["head"]:
                raise ValueError("Author worktree must match the attached PR")
        except Exception:
            state["pr"] = previous
            raise
        state.update(phase="watching", last_snapshot=snapshot, next_poll_at=0)
        persist(state)
        if not arguments.no_schedule:
            scheduler.install(state)
    elif action == "schedule":
        scheduler.install(state)
    elif action == "finalize":
        scheduler.finalize(state)
    elif action == "rebind":
        setup.rebind(state, arguments.socket, arguments.accept_input)
    elif action == "pause":
        if state["phase"] == "complete":
            raise ValueError("Completed tasks cannot be paused")
        state.update(phase="paused", next_poll_at=0)
        persist(state)
        record(state, "paused")
    elif action == "resume":
        if state["phase"] in {"complete", "finalizing"}:
            raise ValueError("Ended tasks cannot resume repair; use finalize for pending cleanup")
        if not 0 <= arguments.additional_rounds <= 20 or not 0 <= arguments.extend_hours <= 720:
            raise ValueError("Explicit budget extensions exceed the supported bounds")
        if arguments.retry_failed:
            event = lifecycle.active(state)
            if not event:
                raise ValueError("No interrupted author event is available")
            receipt = lifecycle.native.delivery(state, event)
            if receipt.get("turn_status") not in {"completed", "failed", "interrupted"}:
                raise ValueError("Retry only a confirmed terminal author turn")
            event["status"] = "superseded"
            state.update(active_event=None, retry_from=event["id"])
        state["max_rounds"] += arguments.additional_rounds
        state["deadline"] += arguments.extend_hours * 3600
        state.update(phase="watching" if state["pr"] else "developing", next_poll_at=0)
        persist(state)
        record(state, "resumed", added_rounds=arguments.additional_rounds, extended_hours=arguments.extend_hours)
    return summary(state)


def main():
    arguments = parser().parse_args()
    if arguments.command == "feedback":
        return publish.feedback(arguments.repository, arguments.pr, arguments.head,
                                arguments.request_id, arguments.body_file.read_text())
    if arguments.command == "init":
        if not arguments.thread:
            raise ValueError("Provide the original author UUID using --thread or CODEX_THREAD_ID")
        return summary(setup.initialize(arguments))
    directory = arguments.state.resolve()
    if arguments.command == "probe":
        state = load(directory)
        setup.verify_checker(state)
        snapshot = forge.snapshot(state)
        if snapshot["state"] != "open":
            if state["phase"] == "finalizing":
                return {"cleanup_only": True, "terminal": snapshot["state"]}
            raise ValueError("Cannot enable monitoring for an ended PR")
        publish.ensure_labels(state["repository"])
        return {"comment": publish.comment_once(
            state["repository"], state["pr"], f"<!-- pr-task:watcher {state['id']} -->",
            "本 PR 已登记项目内作者巡检。只在新反馈时启动修复；合并或关闭后收尾。")}
    with locked(directory, wait=arguments.command == "finalize"):
        state = load(directory)
        try:
            if arguments.command not in {"status", "pause"}:
                setup.verify_checker(state)
            return execute(state, arguments)
        except Exception as error:
            if arguments.command not in {"status", "claim"}:
                delay = error.retry_after if isinstance(error, forge.ForgeError) else 60
                previous = state.get("error_backoff", 30)
                state["error_backoff"] = min(max(delay, previous * 2), 3600)
                defer(state, error, max(delay, state["error_backoff"]),
                      pause=isinstance(error, ValueError) or isinstance(error, forge.ForgeError) and error.permission)
            raise


if __name__ == "__main__":
    try:
        print(json.dumps(main(), ensure_ascii=False))
    except BlockingIOError:
        print(json.dumps({"status": "busy", "detail": "Another task operation owns the lock"}))
        sys.exit(75)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(json.dumps({"status": "error", "type": type(error).__name__, "detail": str(error)},
                         ensure_ascii=False), file=sys.stderr)
        sys.exit(1)
