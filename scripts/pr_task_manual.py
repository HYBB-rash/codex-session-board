from scripts import pr_task_github as forge
from scripts import pr_task_publish as publish
from scripts.pr_task_state import checkout, now, persist, record, summary


def enabled(state):
    return state.get("mode", "automatic") == "manual"


def require_action(state, action):
    if enabled(state) and action not in {"status", "submit", "attach", "ready", "finalize"}:
        raise ValueError("Manual tasks use status --refresh, submit, ready and finalize; no automatic transport")
    if action == "ready" and not enabled(state):
        raise ValueError("Explicit ready is for manual tasks; automatic tasks retain their existing lifecycle")


def observe(state):
    result = summary(state)
    if not state["pr"]:
        result["handoff"] = {"next_action": "submit", "feedback": []}
        return result
    snapshot = forge.snapshot(state)
    if snapshot["state"] != "open":
        next_action = "complete" if state["phase"] == "complete" else "finalize"
    elif state["phase"] == "complete":
        next_action = "register-new-task"
    elif snapshot["items"]:
        next_action = "address-feedback"
    elif state.get("published", {}).get("head") != snapshot["head"]:
        next_action = "submit"
    elif snapshot["ci"] != "passed":
        next_action = "wait-ci"
    else:
        next_action = "independent-review" if (
            state.get("ready_head") == snapshot["head"] and not snapshot["draft"]) else "ready"
    result["handoff"] = {
        "url": snapshot["url"], "head": snapshot["head"], "base": snapshot["base"],
        "state": snapshot["state"], "ci": snapshot["ci"], "draft": snapshot["draft"],
        "feedback": snapshot["items"], "next_action": next_action, "observed_at": now(),
    }
    return result


def ready(state):
    if not state["pr"] or state["phase"] in {"complete", "finalizing"}:
        raise ValueError("Ready requires an open submitted task")
    snapshot = forge.snapshot(state)
    if snapshot["state"] != "open" or snapshot["ci"] != "passed" or snapshot["items"]:
        raise ValueError("Ready requires passed CI and no unresolved feedback on the open PR")
    if (state.get("published", {}).get("head") != snapshot["head"]
            or checkout(state, clean=True) != snapshot["head"]):
        raise ValueError("Submit and validate the current PR head before announcing readiness")
    publish.ready(state, snapshot, refresh=True)
    state.update(phase="awaiting-review", last_snapshot=snapshot)
    persist(state)


def finalize(state):
    if not state["pr"]:
        raise ValueError("Finalize requires a registered PR")
    snapshot = forge.snapshot(state)
    if snapshot["state"] not in {"merged", "closed"}:
        raise ValueError("Finalize only a merged or closed PR; reopened PRs need inspection")
    outcome = {
        "result": "merged" if snapshot["state"] == "merged" else "closed_without_merge",
        "head": snapshot["head"], "merge_commit": snapshot["merge_commit"],
    }
    if state["phase"] == "complete" and all(state.get("outcome", {}).get(key) == value
                                           for key, value in outcome.items()):
        return
    state.update(phase="complete", cleanup_pending=False, last_snapshot=snapshot,
                 outcome={**outcome, "observed_at": now()},
                 archive_receipt={"status": "not_requested"})
    persist(state)
    record(state, "manual_finalized", **outcome)
