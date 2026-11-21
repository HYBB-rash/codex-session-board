import os
from pathlib import Path
import shlex
import time
import uuid

from scripts import pr_task_github as forge
from scripts import pr_task_native as native
from scripts import pr_task_publish as publish
from scripts.pr_task_state import ROOT, checkout, digest, now, persist, record


def active(state):
    return next((entry for entry in state["events"] if entry["id"] == state["active_event"]), None)


def create_event(state, snapshot, items):
    identifier = str(uuid.uuid4())
    command = shlex.join([str(ROOT / "scripts/pr-task"), "claim", "--state", state["directory"],
                          "--event", identifier])
    finish = shlex.join([str(ROOT / "scripts/pr-task"), "complete", "--state", state["directory"],
                         "--event", identifier])
    submit = shlex.join([str(ROOT / "scripts/pr-task"), "submit", "--state", state["directory"]])
    queue_feedback = next((item for item in items if item["kind"] == "feedback"
                           and "<!-- pr-queue:" in item.get("body", "")), None)
    result_notice = (f"PR #{state['pr']} 本轮合并失败。反馈 {queue_feedback['url']}。\n"
                     if queue_feedback else "")
    event = {"id": identifier, "head": snapshot["head"], "items": items, "status": "pending",
             "created_at": now()}
    if state.get("retry_from"):
        event["resume_from"] = state.pop("retry_from")
