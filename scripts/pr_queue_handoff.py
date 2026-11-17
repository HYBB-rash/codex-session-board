from datetime import datetime
from pathlib import Path

from scripts import pr_task_github as forge
from scripts.pr_task_state import now, save


ACTORS = {
    "merged": "user", "closed-without-merge": "user", "waiting-ci": "external-ci",
    "needs-author": "author", "needs-evidence": "author-or-reviewer",
    "infrastructure-blocked": "user", "needs-user": "user", "ready": "coordinator",
}


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
        result = {"number": item["number"], "head": pr["head"]["sha"], "url": pr["html_url"]}
        if pr["merged"]:
            result["result"] = "merged" if contained(repository, pr["merge_commit_sha"], main) else "needs-user"
        elif pr["state"] == "closed":
            result["result"] = "closed-without-merge"
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
    } for item in state["items"]]
    return {
        "result": result, "detail": detail, "next_actor": ACTORS[result],
        "repository": state["repository"], "reference": state.get("reference", "refs/heads/main"),
        "base": state["base"], "candidate": state.get("frozen_candidate"),
        "integration": integration, "items": items, "observation": state.get("observation"),
        "retain": {"queue": str(directory), "project": state["project"], "checker": state["checker"]},
        "continue": "继续合并 " + "、".join(f"PR #{item['number']}" for item in items),
        "release_status": state.get("release_status", "not_requested"),
    }


def resume(state, directory, queue):
    if state.get("feedback_mode") != "github":
        raise ValueError("Manual merge handoff requires github feedback; existing native queues remain unchanged")
    state["observation"] = observe(state)
    result = state["observation"]["result"]
    if result in {"merged", "closed-without-merge"}:
        for observed in state["observation"]["items"]:
            item = next(item for item in state["items"] if item["number"] == observed["number"])
            item["status"] = "merged" if observed["result"] == "merged" else "closed"
        return describe(state, directory, result)
    if result != "ready":
        return describe(state, directory, result, state["observation"]["detail"])
    queue.verify_checker(state)
    for item in state["items"]:
        if item["status"] in {"merged", "closed"}:
            continue
        try:
            queue.verify_entry(state, item)
        except Exception as error:
            return describe(state, directory, "needs-evidence", str(error))
    frozen = state.get("frozen_candidate")
    if frozen:
        queue.clean(Path(state["project"]), frozen["candidate"])
        if frozen["status"] == "failed":
            return describe(state, directory, "needs-author", "Repair or explicitly retry the recorded failed run")
        queue.finish(state, directory)
        return describe(state, directory, state.get("merge_status", "merged"))
    return describe(state, directory, "ready", "Continue mechanical attempt and release freeze using existing commands")


def begin(state, directory, command):
    for previous in state.get("rounds", []):
        if previous["result"] == "running":
            previous.update(result="interrupted", interruption_observed_at=now())
    record = {"command": command, "started_at": now(), "result": "running"}
    state.setdefault("rounds", []).append(record)
    save(directory / "queue.json", state)
    return record


def end(state, directory, record, handoff):
    record.update(finished_at=now(), result=handoff["result"], handoff=handoff)
    record["seconds"] = (datetime.fromisoformat(record["finished_at"])
                         - datetime.fromisoformat(record["started_at"])).total_seconds()
    state["handoff"] = handoff
    save(directory / "queue.json", state)
