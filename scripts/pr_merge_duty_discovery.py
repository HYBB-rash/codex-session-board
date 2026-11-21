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
