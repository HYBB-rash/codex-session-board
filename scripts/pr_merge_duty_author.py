import json
from pathlib import Path

from scripts import pr_queue
from scripts.pr_task_state import digest, git, load


def delivery(config, checker, claim, queue_item=None):
    repository, branch, number, head = (claim[key] for key in ("repository", "source_branch", "pr", "head"))
    approved = (config.get("author_receipts") or {}).get(str(number))
    receipt = None
    if approved:
        path = Path(approved)
        if not path.is_absolute():
            raise ValueError("Use an absolute approved author receipt path")
        receipt = json.loads(path.read_text())
        if (receipt["repository"], receipt["pr"], receipt["branch"], receipt["base"]) != (
                repository, number, branch, "main"):
            raise ValueError("Approved author receipt differs from this claim")
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
        route = task["route"]
        if receipt and "socket" not in route and receipt["route"]["thread_id"] == route["thread_id"]:
            route = receipt["route"]
        return {**task, "route": route, "delivery_head": publication.get("head"),
                "validation": publication.get("validation"), "author_task": str(directory)}
    if receipt:
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
    item["scope"] = {key: claim[key] for key in ("source_repository", "source_branch", "target_branch")}
    if author.get("author_task"):
        item["author_task_state"] = author["author_task"]
    pr_queue.verify_pr_validation(item, checker)
    return item


def review_feedback(claim, review, report):
    receipt = json.loads(Path(review["review"]).read_text())
    if not isinstance(receipt.get("detail"), str) or not receipt["detail"].strip():
        return {**report, "result": "needs-evidence", "detail": "Independent review lacks actionable feedback"}
    return post_feedback(claim, report, ["review-failed"], "review", receipt["detail"])


def post_feedback(claim, report, event, kind, detail):
    import uuid
    from scripts.pr_task_publish import feedback

    key = str(uuid.uuid5(uuid.NAMESPACE_URL, digest(
        [claim["repository"], claim["pr"], claim["head"], *event])))
    posted = feedback(claim["repository"], claim["pr"], claim["head"], key, detail)
    return {**report, "result": "needs-author", "feedback": {
        "id": key, "comment": posted["url"], "kind": kind, "head": claim["head"],
    }}


def ci_feedback(claim, author, report, record):
    run = report["workflow"]
    key = f"{run['run']}:{run['attempt']}"
    events = record.setdefault("ci_feedback", {})
    if author and author.get("author_task") and author.get("mode", "automatic") == "automatic":
        if run["conclusion"] != "failure":
            raise ValueError("CI must have actually failed before relaying author feedback")
        links = {job["url"] for job in run["jobs"]}
        event = next((item for item in author["events"] if item["head"] == claim["head"]
                      and any(check["kind"] == "check" and check.get("url") in links
                              for check in item["items"])), None)
        author_delivery = event.get("delivery") if event else None
        return {**report, "result": "needs-author", "delivery": {
            "author": author_delivery or {"state": "waiting-for-author-tick"}}}
    if key not in events:
        jobs = "\n".join(f"- {job['name']}: {job['url']}" for job in run["jobs"])
        detail = (f"当前提交的 GitHub PR CI 未通过，运行 {run['url']}，第 {run['attempt']} 次尝试。\n"
                  f"{jobs}\n按失败作业中的 `scripts/validate --scope pr` 入口在原分支复现并提交修复。")
        result = post_feedback(claim, report, ["ci", key], "ci", detail)
        events[key] = result["feedback"]
        return result
    return {**report, "result": "needs-author", "feedback": events[key]}


def resolve_ci_feedback(claim, record):
    import hashlib
    from scripts import pr_task_github as forge
    from scripts.pr_task_publish import comment_once

    for event in record.get("ci_feedback", {}).values():
        if event.get("resolution"):
            continue
        repository = claim["repository"]
        comment_id = int(event["comment"].rsplit("-", 1)[1])
        original = forge.api(f"repos/{repository}/issues/comments/{comment_id}")
        body = original["body"]
        head = claim["head"]
        if f"<!-- pr-task:feedback {event['id']} -->" not in body or f"PR head: `{head}`" not in body:
            raise ValueError("Existing CI feedback no longer matches the exact source and request")
        checksum = hashlib.sha256(body.encode()).hexdigest()
        marker = f"<!-- pr-queue:resolved {event['id']} {head} {checksum} -->"
        event["resolution"] = comment_once(repository, claim["pr"], marker,
                                           "当前头的 GitHub PR CI 已通过，原 CI 失败记录保留。")
