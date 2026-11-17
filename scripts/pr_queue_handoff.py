from datetime import datetime
from pathlib import Path
import time

from scripts import pr_task_github as forge
from scripts.pr_task_state import now, save


ACTORS = {
    "merged": "user", "closed-without-merge": "user", "waiting-ci": "external-ci",
    "needs-author": "author", "needs-evidence": "author-or-reviewer",
    "infrastructure-blocked": "user", "needs-user": "user", "ready": "coordinator",
}
NEXT = {
    "merged": "report-main-and-release-status", "closed-without-merge": "report-terminal-state",
    "waiting-ci": "return-and-resume-on-user-request", "needs-author": "hand-feedback-to-author",
    "needs-evidence": "register-current-head-review-and-validation",
    "infrastructure-blocked": "resolve-infrastructure-and-resume", "needs-user": "inspect-recorded-blocker",
    "ready": "continue-existing-queue-commands",
}


def first_ready(repository, number, pr):
    events = forge.pages(f"repos/{repository}/issues/{number}/timeline", time.monotonic() + 90)
    changes = sorted((entry for entry in events if entry["event"] in
                      {"ready_for_review", "convert_to_draft"}), key=lambda entry: entry["created_at"])
    if changes and changes[0]["event"] == "ready_for_review":
        return changes[0]["created_at"]
    return pr["created_at"] if changes or not pr["draft"] else None


def contained(repository, source, main):
    if source == main:
        return True
    comparison = forge.api(f"repos/{repository}/compare/{source}...{main}")
    return comparison["status"] in {"ahead", "identical"}


def observe(state):
    repository = state["repository"]
    reference = state.get("reference", "refs/heads/main")
    main = forge.api(f"repos/{repository}/git/ref/{reference.removeprefix('refs/')}")["object"]["sha"]
    results = []
    for item in state["items"]:
        pr = forge.api(f"repos/{repository}/pulls/{item['number']}")
        ready = item.get("first_ready_at")
        if not ready and pr.get("created_at"):
            ready = first_ready(repository, item["number"], pr)
        result = {"number": item["number"], "head": pr["head"]["sha"], "url": pr["html_url"],
                  "checks_url": pr["html_url"] + "/checks", "first_ready_at": ready,
                  "merged_at": pr.get("merged_at")}
        if pr["merged"]:
            result["result"] = "merged" if contained(repository, pr["merge_commit_sha"], main) else "needs-user"
        elif pr["state"] == "closed":
            result["result"] = "closed-without-merge"
        elif item["status"] in {"closed", "merged"}:
            result.update(result="needs-user", detail="PR reopened after terminal handoff; inspect before registering again")
        elif pr["base"]["ref"] != reference.removeprefix("refs/heads/"):
            result.update(result="needs-user", detail="PR target changed")
        elif pr["head"]["sha"] != item["head"]:
            result.update(result="needs-evidence", detail="PR head changed; requeue its exact new evidence")
        else:
            snapshot = forge.snapshot({
                "repository": repository, "pr": item["number"], "branch": pr["head"]["ref"],
                "base": pr["base"]["ref"], "checks": item.get("checks", ["changes"]),
                "reviewers": item.get("reviewers", [repository.split("/")[0]]),
            })
            if snapshot["head"] != item["head"] or snapshot["state"] != "open":
                raise ValueError("PR changed while refreshing the handoff; refresh again")
            result.update(snapshot=snapshot, result="ready")
            if snapshot["items"] or snapshot["ci"] == "failed":
                result["result"] = "needs-author"
            elif item.get("feedback"):
                result["result"] = "needs-evidence"
            elif item["status"] == "blocked-dependency":
                result["result"] = "needs-author"
            elif snapshot["ci"] != "passed":
                result["result"] = "waiting-ci"
        results.append(result)
    terminal = all(item["result"] in {"merged", "closed-without-merge"} for item in results)
    result = "merged" if all(item["result"] == "merged" for item in results) else (
        "closed-without-merge" if terminal else "ready")
    if not terminal and main != state["base"]:
        result = "needs-user"
    elif not terminal:
        result = next((item["result"] for item in results if item["result"] != "ready"), "ready")
    return {"result": result, "main": main, "items": results, "observed_at": now(),
            "detail": "Main advanced; recombine and validate" if not terminal and main != state["base"] else ""}


def describe(state, directory, result, detail=""):
    integration = state.get("integration", {})
    items = [{
        "number": item["number"], "head": item["head"], "base": item["base"],
        "review": item["review"], "validation": item["validation"], "status": item["status"],
        "feedback": item.get("feedback"), "author_task_state": item.get("author_task_state"),
        "first_ready_at": item.get("first_ready_at"), "merged_at": item.get("merged_at"),
        "ready_to_main_seconds": (
            datetime.fromisoformat(item["merged_at"].replace("Z", "+00:00"))
            - datetime.fromisoformat(item["first_ready_at"].replace("Z", "+00:00"))).total_seconds()
        if item.get("merged_at") and item.get("first_ready_at") else None,
    } for item in state["items"]]
    return {
        "result": result, "detail": detail, "next_actor": ACTORS[result], "next_action": NEXT[result],
        "repository": state["repository"], "reference": state.get("reference", "refs/heads/main"),
        "base": state["base"], "candidate": state.get("frozen_candidate"),
        "integration": integration, "items": items, "observation": state.get("observation"),
        "retain": {"queue": str(directory), "project": state["project"], "checker": state["checker"],
                   "checker_project": state.get("checker_project")},
        "continue": "继续合并 " + "、".join(f"PR #{item['number']}" for item in items),
        "release_status": state.get("release_status", "not_requested"),
    }


def refresh(state):
    state["observation"] = observe(state)
    for observed in state["observation"]["items"]:
        item = next(item for item in state["items"] if item["number"] == observed["number"])
        for field in ("first_ready_at", "merged_at"):
            if observed.get(field):
                item[field] = observed[field]
    state["resume_rechecked"] = ["GitHub PR/head/CI/feedback", "main"]
    return state["observation"]


def resume(state, directory, queue):
    if state.get("feedback_mode") != "github":
        raise ValueError("Manual merge handoff requires github feedback; existing native queues remain unchanged")
    refresh(state)
    result = state["observation"]["result"]
    if result in {"merged", "closed-without-merge"}:
        for observed in state["observation"]["items"]:
            item = next(item for item in state["items"] if item["number"] == observed["number"])
            item["status"] = "merged" if observed["result"] == "merged" else "closed"
        return describe(state, directory, result)
    if state["observation"]["main"] != state["base"]:
        report = describe(state, directory, result, state["observation"]["detail"])
        report.update(next_actor="coordinator", next_action="recombine-and-validate")
        return report
    accepted = {item["number"] for item in state["items"] if item["status"] == "accepted"}
    recombining = {item["number"] for item in state["items"] if item.get("recombine_required")}
    relevant = [item for item in state["observation"]["items"]
                if not accepted or item["number"] in accepted | recombining]
    blocker = next((item for item in relevant if item["result"] not in {"ready", "waiting-ci"}), None)
    if blocker:
        if blocker["result"] in {"merged", "closed-without-merge"}:
            return describe(state, directory, "needs-user", "Part of the candidate became terminal; recombine")
        return describe(state, directory, blocker["result"], blocker.get("detail", ""))
    queue.verify_checker(state)
    state["resume_rechecked"] += ["frozen-checker", "source-bound-review-and-validation"]
    for item in state["items"]:
        if accepted and item["number"] not in accepted | recombining or item["status"] in {"merged", "closed"}:
            continue
        try:
            queue.verify_entry(state, item)
        except Exception as error:
            return describe(state, directory, "needs-evidence", str(error))
    if recombining:
        return describe(state, directory, "ready", "Use the existing mechanical attempt to recombine changed PR heads")
    if not accepted:
        return describe(state, directory, "ready",
                        "Continue mechanical attempt; PR CI can run alongside candidate validation")
    project = Path(state["project"])
    candidate = queue.git(project, "rev-parse", "HEAD")
    queue.clean(project, candidate)
    frozen = state.get("frozen_candidate", {})
    if frozen.get("status") == "failed" and candidate == frozen["candidate"]:
        return describe(state, directory, "needs-author", "Repair or explicitly retry the recorded failed run")
    if candidate == state["prefix"] and candidate != frozen.get("candidate"):
        return describe(state, directory, "ready", "Freeze the accepted prefix using the existing release command")
    state["resume_reused"] = ["independent-review", "submit-validation", "accepted-prefix"]
    if frozen.get("candidate") == candidate:
        state["resume_reused"] += ["frozen-candidate", "existing-workflow"]
    queue.finish(state, directory)
    result = state["merge_status"]
    if result == "merged" and not state["all_items_merged"]:
        result = "needs-author"
    return describe(state, directory, result)


def begin(state, directory, command):
    for previous in state.get("rounds", []):
        if previous["result"] == "running":
            previous.update(result="interrupted", interruption_observed_at=now())
    record = {"command": command, "started_at": now(), "result": "running"}
    previous = state.get("rounds", [])[-1] if state.get("rounds") else {}
    if previous.get("finished_at"):
        record["between_calls"] = {
            "from": previous["finished_at"], "to": record["started_at"], "reason": previous["result"],
            "seconds": (datetime.fromisoformat(record["started_at"])
                        - datetime.fromisoformat(previous["finished_at"])).total_seconds(),
        }
    state.setdefault("rounds", []).append(record)
    save(directory / "queue.json", state)
    return record


def end(state, directory, record, handoff):
    record.update(finished_at=now(), result=handoff["result"], handoff=handoff)
    record["seconds"] = (datetime.fromisoformat(record["finished_at"])
                         - datetime.fromisoformat(record["started_at"])).total_seconds()
    state["handoff"] = handoff
    state["merge_status"] = handoff["result"]
    record["reused"] = state.pop("resume_reused", [])
    record["rechecked"] = state.pop("resume_rechecked", [])
    if record.get("request_started_at"):
        record["request_seconds"] = (datetime.fromisoformat(record["finished_at"])
                                     - datetime.fromisoformat(record["request_started_at"])).total_seconds()
    save(directory / "queue.json", state)
