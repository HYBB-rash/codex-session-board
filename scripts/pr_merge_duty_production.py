import json
from pathlib import Path
import subprocess

from scripts import pr_merge_duty_author as authors
from scripts import pr_merge_duty_discovery as discovery
from scripts import pr_merge_duty_queue as queues
from scripts import pr_queue_gates as gates
from scripts import pr_queue_handoff as handoff
from scripts import pr_queue_registry as registry
from scripts import pr_task_github as forge
from scripts.pr_merge_duty_delivery import dispatch
from scripts.pr_task_state import now, save


def terminal(duty, config, directory, claim, main, queue, state, observed):
    if not observed["github_merged"]:
        return {"result": "closed-without-merge", "remote_main": main}
    number = claim["pr"]
    if not observed["merge_commit"] or not handoff.contained(config["repository"], observed["merge_commit"], main):
        return {"result": "waiting-pr-terminal", **{key: observed[key] for key in ("github_merged", "merge_commit")}}
    result = {"result": "merged", "remote_main": main, "merge_commit": observed["merge_commit"],
              "combined_verified": False}
    if state and state.get("frozen_candidate") and state["items"][0]["head"] == observed["observed_head"]:
        item = state["items"][0]
        assignment = {"pr": number, "head": item["head"], "candidate": state["project"]}
        recorded = duty.advance(config, Path(state["checker_project"]), directory, assignment,
                                {"result": "closed", "github_merged": True}, queue)
        if recorded["result"] == "merged" and recorded.get("candidate", {}).get("status") == "merged":
            result.update(recorded, combined_verified=True)
    return result


def author_result(config, directory, claim, author, result):
    route = (author or {}).get("route") or claim.get("author")
    if not route:
        return {"author": {"state": "unbound"}}
    assignment = {"pr": claim["pr"], "head": claim["head"]}
    if author and author.get("author_task"):
        assignment["author_task"] = author["author_task"]
    try:
        return dispatch(config, directory, assignment, result, route=route)
    except (ValueError, OSError, KeyError) as error:
        return {"author": {"state": "pending", "detail": str(error)}}


def bind_author(claim, author):
    if author:
        route = author["route"]
        if claim.get("author") and claim["author"]["thread_id"] != route["thread_id"]:
            raise ValueError("Original author session changed; coordinator must verify the route")
        claim["author"] = route
    return author


def follow(duty, config, checker, directory, path, data, claim, main):
    repository, number = claim["repository"], claim["pr"]
    pull = forge.api(f"repos/{repository}/pulls/{number}")
    observed = discovery.refresh_claim(claim, pull)
    discovery.remember_legacy_review(directory, claim, config.get("coordinator_thread"))
    registry_project = Path(config.get("registry_project", checker))
    queue = queues.locate(registry_project, claim, directory)
    state = queues.read_queue(queue, claim) if queue else None
    if state:
        if config.get("registry_project"):
            registry.register(queue, state, project=registry_project)
        claim["queue"] = str(queue)
    old_item = state["items"][0] if state else None
    save(path, data)
    if observed:
        if observed["result"] != "observing-terminal":
            return observed
        result = terminal(duty, config, directory, claim, main, queue, state, observed)
        if result["result"] == "merged":
            try:
                author = bind_author(claim, authors.delivery(config, registry_project, claim, old_item))
                result["delivery"] = author_result(config, directory, claim, author, result)
            except (ValueError, OSError, KeyError) as error:
                result["delivery"] = {"author": {"state": "pending", "detail": str(error)}}
        return result

    head = claim["head"]
    author = bind_author(claim, authors.delivery(config, registry_project, claim, old_item))
    author_id = author["route"]["thread_id"] if author else None
    receipt = discovery.review_path(directory, number, head)
    if old_item and old_item["head"] == head and not receipt.exists():
        original = Path(old_item["review"])
        if original.is_file():
            save(receipt, json.loads(original.read_text()))
    save(path, data)
    review = discovery.review_state(directory, claim, author_id, config.get("coordinator_thread"))
    if review["result"] in {"review-passed", "review-failed"} and review["request"] is None:
        claim.setdefault("review_task", review["reviewer"])
        save(path, data)
    report = {"review": review, "author_evidence": bool(author and author.get("delivery_head") == head)}
    if review["result"] == "review-failed":
        previous = claim["heads"][head].get("feedback")
        if previous:
            result = {**report, "result": "needs-author", "feedback": previous}
        else:
            result = authors.review_feedback(claim, review, report)
            if result.get("feedback"):
                claim["heads"][head]["feedback"] = result["feedback"]
                save(path, data)
        if result.get("feedback"):
            result["delivery"] = author_result(config, directory, claim, author, result)
        return result
    try:
        ci = gates.workflow(repository, number, head, include_pending=True)
    except gates.GateBlocked as error:
        if review["result"] == "review-passed":
            return {**report, "result": error.result, "detail": str(error)}
        report["ci_error"] = str(error)
        ci = None
    report["workflow"] = ci
    head_record = claim["heads"][head]
    if ci:
        runs = head_record.setdefault("ci_runs", {})
        runs[str(ci["run"])] = ci
        save(path, data)
    elif head_record.get("ci_runs"):
        report["previous_ci_runs"] = list(head_record["ci_runs"].values())
    if review["result"] != "review-passed":
        return {**report, "result": review["result"]}
    if ci and ci.get("status") == "completed":
        if ci["conclusion"] != "failure" or not ci["jobs"]:
            return {**report, "result": "infrastructure-blocked", "detail": "PR workflow ended without usable failure logs"}
        result = authors.ci_feedback(claim, author, report, head_record)
        save(path, data)
        if result.get("feedback"):
            result["delivery"] = author_result(config, directory, claim, author, result)
        return result
    if ci is None or ci.get("status"):
        return {**report, "result": "waiting-ci"}
    if not report["author_evidence"]:
        return {**report, "result": "needs-evidence", "detail": "Current-head author submit evidence is missing"}
    if author.get("archive"):
        return {**report, "result": "needs-user", "detail": "Author archive could precede result delivery"}
    checker_id = state["checker"] if state else author["checker"]
    item = authors.checked_item(claim, author, checker_id, review["review"])
    authors.resolve_ci_feedback(claim, head_record)
    save(path, data)
    snapshot = forge.snapshot({**author, "repository": repository, "pr": number, "branch": claim["source_branch"],
                               "base": "main", "checks": item["checks"], "reviewers": item["reviewers"]})
    if snapshot["head"] != head or snapshot["state"] != "open" or snapshot["draft"]:
        raise forge.ForgeError("Claim changed during readback; repeat with a fresh snapshot")
    if snapshot["items"]:
        previous = (old_item or {}).get("feedback") or {}
        result = {**report, "result": "needs-author", "feedback": previous if previous.get("head") == head else None,
                  "github_feedback": snapshot["items"]}
        result["delivery"] = author_result(config, directory, claim, author, result)
        return result
    if snapshot["ci"] != "passed":
        return {**report, "result": "waiting-ci"}
    result = {**report, **queues.advance(duty, config, checker, directory, claim, author, item, main, queue, state)}
    if result.get("queue"):
        claim["queue"] = result["queue"]
        save(path, data)
    result["delivery"] = author_result(config, directory, claim, author, result)
    return result


def worklist(data, pulls):
    return sorted((claim for claim in data["claims"].values()
                   if claim["status"] not in discovery.TERMINAL or claim["pr"] in pulls
                   or (claim.get("last_result", {}).get("delivery", {}).get("author", {}).get("state")
                       in {"pending", "waiting_idle", "archived", "sending", "sent", "delivery_unknown",
                           "rejected", "author_failed", "unbound"})),
                  key=lambda claim: claim["pr"])


def run(duty, config, checker, directory, arguments):
    repository = config["repository"]
    identity = {"repository": repository, "source": config["checker_head"], "at": now()}
    path, data = discovery.load_claims(directory, repository)
    if arguments.command in {"reserve-review", "begin-review", "record-review", "recover-review"}:
        return {**identity, **discovery.review_command(directory, path, data, arguments,
                                                       config.get("coordinator_thread"))}
    report = directory / ("last-scan.json" if arguments.command == "scan" else "last-tick.json")
    try:
        pending, pulls = discovery.discover(data, discovery.open_pulls(repository))
        main = forge.api(f"repos/{repository}/git/ref/heads/main")["object"]["sha"]
        if not discovery.SHA.fullmatch(main):
            raise forge.ForgeError("Remote main did not return a complete commit")
        if not handoff.contained(repository, config["checker_head"], main):
            raise forge.ForgeError("The configured production duty source does not belong to remote main")
        active = worklist(data, pulls)
        current = {**identity, "main": main, "pending": pending,
                   "active": [{key: entry[key] for key in ("pr", "head", "status")} for entry in active]}
        if arguments.command == "tick" and (pending or active):
            claim = None
            if pending and (not active or not data.get("prefer_existing")):
                confirmed = discovery.confirm_ready(repository, pending[0])
                if confirmed is None:
                    raise forge.ForgeError("PR readiness changed before first claim; refresh the full query")
                claim = discovery.claim_one(path, data, [confirmed])
            if claim:
                current["claimed"] = {key: claim[key] for key in ("pr", "head", "source_branch")}
                current["pending"] = pending[1:]
            else:
                claim = next((entry for entry in active if entry["pr"] > data.get("cursor", 0)), active[0])
            data["prefer_existing"] = "claimed" in current
            data["cursor"] = claim["pr"]
            save(path, data)
            try:
                outcome = follow(duty, config, checker, directory, path, data, claim, main)
            except BlockingIOError:
                outcome = {"result": "busy", "detail": "Existing queue operation owns the lock"}
            except forge.ForgeError as error:
                outcome = {"result": "infrastructure-blocked", "detail": str(error),
                           "retry_after": error.retry_after}
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                outcome = {"result": "needs-user", "detail": str(error)}
            claim.update(status=outcome["result"], last_result=outcome)
            save(path, data)
            current["item"] = {"pr": claim["pr"], "head": claim["head"], "url": claim["url"], **outcome}
        current["result"] = ("infrastructure-blocked"
                             if (current.get("item") or {}).get("result") == "infrastructure-blocked"
                             else "scanned" if pending or active else "idle")
    except (forge.ForgeError, KeyError, TypeError, TimeoutError, subprocess.SubprocessError) as error:
        current = {**identity, "result": "infrastructure-blocked", "detail": str(error),
                   "retry_after": getattr(error, "retry_after", 60)}
    if current["result"] == "infrastructure-blocked":
        save(directory / "last-error.json", current)
    save(report, current)
    return current
