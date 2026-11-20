import json
from pathlib import Path

from scripts import pr_queue
from scripts.pr_task_state import digest, git, load


def delivery(config, checker, claim, queue_item=None):
    repository, branch, number, head = (claim[key] for key in ("repository", "source_branch", "pr", "head"))
    common = Path(git(checker, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    registry = common / "pr-tasks" / f"{digest([repository, branch])}.json"
    if registry.exists():
        registration = json.loads(registry.read_text())
        directory = Path(registration["directory"]).resolve(strict=True)
        task = load(directory)
        if ((task["repository"], task["branch"], task["base"], task["pr"], task["route"]["thread_id"])
                != (repository, branch, "main", number, registration["thread"])):
            raise ValueError("Registered author task differs from this claim")
        publication = task.get("published") or {}
        return {**task, "delivery_head": publication.get("head"), "validation": publication.get("validation"),
                "author_task": str(directory)}
    approved = (config.get("author_receipts") or {}).get(str(number))
    if approved:
        path = Path(approved)
        if not path.is_absolute():
            raise ValueError("Use an absolute approved author receipt path")
        receipt = json.loads(path.read_text())
        if (receipt["repository"], receipt["pr"], receipt["branch"], receipt["base"]) != (
                repository, number, branch, "main"):
            raise ValueError("Approved author receipt differs from this claim")
        return receipt
    if queue_item and queue_item.get("head") == head:
        return {"route": queue_item["route"], "delivery_head": head, "validation": queue_item["validation"],
                "base_commit": queue_item["base"], "checks": queue_item.get("checks", ["changes"]),
                "reviewers": queue_item.get("reviewers", [repository.split("/")[0]])}
    return None


def checked_item(claim, author, checker, review):
    head = claim["head"]
    if not author or author.get("delivery_head") != head or not author.get("validation"):
        raise ValueError("Author submit evidence for the current PR head is missing")
    validation = Path(author["validation"]).resolve(strict=True)
    if validation.is_file():
        if validation.name != "VALIDATION.json":
            raise ValueError("Use the existing submit report or its directory")
        validation = validation.parent
    item = {
        "number": claim["pr"], "head": head, "base": author["base_commit"], "dependencies": [],
        "review": review, "validation": str(validation), "route": author["route"],
        "reviewers": author.get("reviewers", [claim["repository"].split("/")[0]]),
        "checks": author.get("checks", ["changes"]),
    }
    if author.get("author_task"):
        item["author_task_state"] = author["author_task"]
    pr_queue.verify_pr_validation(item, checker)
    return item


def review_feedback(claim, review, report):
    import uuid
    from scripts.pr_task_publish import feedback

    receipt = json.loads(Path(review["review"]).read_text())
    if not isinstance(receipt.get("detail"), str) or not receipt["detail"].strip():
        return {**report, "result": "needs-evidence", "detail": "Independent review lacks actionable feedback"}
    key = str(uuid.uuid5(uuid.NAMESPACE_URL, digest(
        [claim["repository"], claim["pr"], claim["head"], "review-failed"])))
    posted = feedback(claim["repository"], claim["pr"], claim["head"], key, receipt["detail"])
    return {**report, "result": "needs-author", "feedback": {
        "id": key, "comment": posted["url"], "kind": "review", "head": claim["head"],
    }}
