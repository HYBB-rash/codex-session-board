from datetime import datetime
from pathlib import Path
import subprocess
import sys
import time

from scripts.pr_task_state import ROOT, load, now


def author(state, item):
    registered = item.get("author_task_state")
    if not registered:
        return {"number": item["number"], "status": "not_registered"}
    directory = Path(registered).resolve(strict=True)
    task = load(directory)
    if (task["repository"] != state["repository"] or task["pr"] != item["number"]
            or task["route"]["thread_id"] != item["route"]["thread_id"]
            or task["checker"] != state["checker"]
            or task.get("published", {}).get("head") != item["head"]
            or Path(task["project"]).resolve() != Path(item["route"]["repair_cwd"]).resolve()):
        raise ValueError("Author cleanup must match the registered task, source, route and checker")
    if task["phase"] != "complete":
        result = subprocess.run([
            sys.executable, str(ROOT / "scripts/pr_task.py"), "tick", "--force", "--terminal-only",
            "--state", str(directory),
        ], cwd=ROOT, text=True, capture_output=True, timeout=150)
        if result.returncode not in (0, 75):
            raise RuntimeError(f"Author cleanup failed: {result.stderr[-2000:]}")
    deadline = time.monotonic() + 60
    while True:
        task = load(directory)
        if task["phase"] == "complete" or not task.get("cleanup_pending") or time.monotonic() >= deadline:
            break
        time.sleep(1)
    complete = task["phase"] == "complete" and not task.get("cleanup_pending")
    return {"number": item["number"], "state": str(directory), "status": "complete" if complete else "pending",
            "phase": task["phase"], "outcome": task.get("outcome"), "scheduler": task.get("scheduler"),
            "archive_receipt": task.get("archive_receipt")}


def closeout(state):
    if not (state.get("batch_merged") or state.get("batch_delivered")):
        raise ValueError("Close out only after the validated candidate entered main")
    started = time.monotonic()
    previous = state.get("closeout", {})
    record = {"started_at": previous.get("started_at", now()), "authors": []}
    for item in state["items"]:
        if item["status"] != "merged":
            continue
        try:
            result = author(state, item)
        except Exception as error:
            result = {"number": item["number"], "status": "pending", "error": str(error)}
        record["authors"].append(result)
    finished = now()
    record.update(finished_at=finished, attempt_seconds=time.monotonic() - started,
                  seconds=(datetime.fromisoformat(finished) - datetime.fromisoformat(record["started_at"])).total_seconds(),
                  complete=bool(record["authors"])
                  and all(item["status"] == "complete" for item in record["authors"]))
    state["closeout"] = record
    return record
