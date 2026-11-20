import json
from pathlib import Path
import re
import time

from scripts import pr_task_github as forge
from scripts.pr_task_state import digest, now, save


TERMINAL = {"merged", "closed-without-merge"}
SHA = re.compile(r"[0-9a-f]{40}")


def describe(pr):
    source = pr["head"]["repo"]
    return {
        "pr": pr["number"], "url": pr["html_url"], "head": pr["head"]["sha"],
        "source_repository": source["full_name"] if source else None,
        "source_branch": pr["head"]["ref"], "target_branch": pr["base"]["ref"],
        "target_repository": pr["base"]["repo"]["full_name"],
    }


def open_pulls(repository):
    rows = forge.pages(f"repos/{repository}/pulls?state=open&base=main&sort=created&direction=asc",
                      time.monotonic() + 90)
    numbers = set()
    for row in rows:
        entry = describe(row)
        number = entry["pr"]
        if (type(number) is not int or number <= 0 or number in numbers
                or not SHA.fullmatch(entry["head"]) or row["state"] != "open"
                or entry["target_repository"].lower() != repository.lower()
                or entry["target_branch"] != "main"):
            raise forge.ForgeError("Open PR pagination changed or returned an invalid entry; retry the full query")
        numbers.add(number)
    return rows


def ready(row, repository):
    source = row["head"]["repo"]
    return (source is not None and source["full_name"].lower() == repository.lower()
            and not row["draft"] and any(label["name"] == "ready-for-review" for label in row["labels"]))


def load_claims(directory, repository):
    path = directory / "claims.json"
    data = json.loads(path.read_text()) if path.exists() else {"repository": repository, "claims": {}}
    if data["repository"] != repository:
        raise ValueError("Claim ledger belongs to a different repository")
    return path, data


def discover(data, rows):
    claims = data["claims"]
    eligible = [describe(row) for row in rows if ready(row, data["repository"])]
    pending = [entry for entry in eligible if str(entry["pr"]) not in claims]
    return pending, {row["number"]: row for row in rows}


def claim_one(path, data, pending):
    if not pending:
        return None
    entry = pending[0]
    claim = {**entry, "repository": data["repository"], "status": "claimed", "claimed_at": now(),
             "heads": {entry["head"]: {"seen_at": now()}}, "queue": None}
    data["claims"][str(entry["pr"])] = claim
    save(path, data)
    return claim


def confirm_ready(repository, entry):
    pull = forge.api(f"repos/{repository}/pulls/{entry['pr']}")
    current = describe(pull)
    if (current["pr"] == entry["pr"] and pull["state"] == "open" and ready(pull, repository)
            and current["target_repository"] == repository and current["target_branch"] == "main"
            and SHA.fullmatch(current["head"])):
        return current
    return None


def refresh_claim(claim, pull):
    entry = describe(pull)
    fields = ("source_repository", "source_branch", "target_repository", "target_branch")
    if any(entry[field] != claim[field] for field in fields):
        return {"result": "needs-user", "detail": "Claim scope changed", "observed": entry}
    if pull["state"] != "open":
        return {"result": "observing-terminal", "github_merged": pull["merged"],
                "merge_commit": pull["merge_commit_sha"], "observed_head": entry["head"]}
    if claim["status"] in TERMINAL:
        return {"result": "needs-user", "detail": "A terminal claim reopened; confirm its scope"}
    if not SHA.fullmatch(entry["head"]):
        raise forge.ForgeError("Claim refresh did not provide a complete PR head")
    claim["head"] = entry["head"]
    claim["heads"].setdefault(entry["head"], {"seen_at": now()})
    if pull["draft"]:
        return {"result": "waiting-author", "detail": "Author returned the PR to draft"}
    return None


def review_path(directory, number, head):
    return directory / f"review-{number}-{head}.json"


def review_state(directory, claim, author=None, coordinator=None):
    head = claim["head"]
    path = review_path(directory, claim["pr"], head)
    request = claim["heads"][head].get("review")
    receipt = json.loads(path.read_text()) if path.exists() else None
    if (receipt and receipt.get("head") == head and receipt.get("reviewer")
            and author and receipt["reviewer"] not in {author, coordinator}
            and (request is None or request.get("task") == receipt["reviewer"])
            and isinstance(receipt.get("passed"), bool)):
        return {"result": "review-passed" if receipt["passed"] else "review-failed",
                "review": str(path), "reviewer": receipt["reviewer"], "request": request}
    key = digest([claim["repository"].lower(), claim["pr"], head, "independent-review"])
    waiting = request and (request.get("dispatch_started_at") or request.get("task"))
    return {"result": "waiting-review" if waiting else "review-required", "review": str(path),
            "request": request, "request_id": key}


def review_command(directory, path, data, arguments, coordinator):
    claim = data["claims"].get(str(arguments.pr))
    if not claim or arguments.head not in claim["heads"]:
        raise ValueError("Review command must identify a recorded full PR head")
    head = claim["heads"][arguments.head]
    if arguments.command in {"reserve-review", "begin-review"}:
        pull = forge.api(f"repos/{claim['repository']}/pulls/{claim['pr']}")
        if refresh_claim(claim, pull) or claim["head"] != arguments.head:
            raise ValueError("Claim changed before review dispatch; refresh the round")
        state = review_state(directory, claim, (claim.get("author") or {}).get("thread_id"), coordinator)
        if state["result"] != "review-required":
            return state
        request = head.setdefault("review", {"id": state["request_id"], "reserved_at": now()})
        if arguments.command == "begin-review":
            if not state["request"]:
                raise ValueError("Reserve the review before beginning dispatch")
            request["dispatch_started_at"] = now()
        save(path, data)
        return {**state, "result": "dispatch-review" if arguments.command == "begin-review" else "review-reserved",
                "request": request}
    if arguments.command == "recover-review":
        request = head.get("review")
        if not request or request.get("task") or not request.get("dispatch_started_at") or not arguments.recovery_proof:
            raise ValueError("Recovery requires a verified unsent request without a known reviewer")
        request.setdefault("recoveries", []).append({"at": now(), "proof": arguments.recovery_proof,
                                                    "previous_start": request.pop("dispatch_started_at")})
        save(path, data)
        return {"result": "review-reserved", "request": request}
        return state
    request = head.get("review")
    if (not request or not request.get("dispatch_started_at") or not arguments.review_task
            or arguments.review_task in {coordinator, (claim.get("author") or {}).get("thread_id")}):
        raise ValueError("Retain the reserved request and record its actual reviewer task")
    if request.get("task") not in {None, arguments.review_task}:
        raise ValueError("A reviewer task is already recorded for this exact head")
    request["task"] = arguments.review_task
    save(path, data)
    return {"result": "waiting-review", "request": request,
            "review": str(review_path(directory, claim["pr"], arguments.head))}
