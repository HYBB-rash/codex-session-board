import json
import re
import subprocess
import time

from scripts.promote_candidate import github
from scripts.pr_task_state import digest


class ForgeError(RuntimeError):
    def __init__(self, message, status=0, headers=None):
        super().__init__(message)
        headers = headers or {}
        self.permission = status in (401, 403, 404) and not (
            headers.get("x-ratelimit-remaining") == "0" or "retry-after" in headers)
        self.retry_after = max(60, int(headers.get("retry-after", "60")))
        if headers.get("x-ratelimit-remaining") == "0":
            self.retry_after = max(self.retry_after,
                                   int(headers.get("x-ratelimit-reset", "0")) - int(time.time()))


def response(text):
    header, separator, body = text.replace("\r\n", "\n").partition("\n\n")
    if not separator or not header.startswith("HTTP/"):
        raise ForgeError("GitHub did not return a complete HTTP response")
    headers = {key.lower(): value.strip() for line in header.splitlines()[1:]
               for key, colon, value in [line.partition(":")] if colon}
    return int(header.splitlines()[0].split()[1]), headers, json.loads(body)


def api(endpoint, *arguments, deadline=None):
    timeout = 60 if deadline is None else max(0.1, min(60, deadline - time.monotonic()))
    if deadline is not None and time.monotonic() >= deadline:
        raise ForgeError("GitHub snapshot exceeded its time budget")
    try:
        raw = github("api", "--include", endpoint, *arguments, timeout=timeout)
        status, headers, value = response(raw)
    except subprocess.CalledProcessError as error:
        try:
            status, headers, value = response(error.output)
        except (ValueError, ForgeError):
            raise ForgeError("GitHub command failed without a complete response") from error
        raise ForgeError(str(value.get("message", "GitHub request failed")), status, headers) from error
    if status >= 400 or isinstance(value, dict) and value.get("errors"):
        raise ForgeError(str(value), status, headers)
    return value


def pages(endpoint, deadline, field=None):
    result = []
    separator = "&" if "?" in endpoint else "?"
    for number in range(1, 101):
        value = api(f"{endpoint}{separator}per_page=100&page={number}", deadline=deadline)
        entries = value[field] if field else value
        if not isinstance(entries, list):
            raise ForgeError("GitHub pagination returned an invalid collection")
        result.extend(entries)
        if len(entries) < 100:
            return result
    raise ForgeError("GitHub pagination exceeded 100 pages; no partial snapshot accepted")


def thread_roots(repository, number, deadline):
    owner, name = repository.split("/")
    query = """query($owner:String!,$name:String!,$number:Int!,$cursor:String){
      repository(owner:$owner,name:$name){pullRequest(number:$number){
        reviewThreads(first:100,after:$cursor){nodes{id isResolved isOutdated
          comments(first:1){nodes{databaseId}}} pageInfo{hasNextPage endCursor}}
      }}}"""
    roots, cursor, seen = {}, None, set()
    for _ in range(100):
        variables = {"owner": owner, "name": name, "number": number, "cursor": cursor}
        data = graphql(query, variables, deadline)
        page = data["repository"]["pullRequest"]["reviewThreads"]
        for thread in page["nodes"]:
            comments = thread["comments"]["nodes"]
            if not comments or comments[0]["databaseId"] in roots:
                raise ForgeError("Review thread roots are missing or duplicated")
            roots[comments[0]["databaseId"]] = thread
        if not page["pageInfo"]["hasNextPage"]:
            return roots
        cursor = page["pageInfo"]["endCursor"]
        if not cursor or cursor in seen:
            raise ForgeError("Review thread pagination repeated a cursor")
        seen.add(cursor)
    raise ForgeError("Review thread pagination exceeded its bound")


def graphql(query, variables, deadline):
    arguments = ["-f", f"query={query}"]
    for key, value in variables.items():
        if value is not None:
            arguments += ["-F", f"{key}={value}"]
    return api("graphql", *arguments, deadline=deadline)["data"]


def item(kind, entry, head=None):
    body = entry.get("body", "")
    key = digest([kind, str(entry["id"]), body])
    return {"key": key, "kind": kind, "id": str(entry["id"]), "body": body,
            "url": entry.get("html_url", ""), "head": head,
            "author": entry.get("user", {}).get("login")}


def feedback(pr, reviews, comments, inline, roots, reviewers):
    result = []
    published = {entry["id"]: entry for entry in reviews if entry["state"] != "PENDING"}
    for entry in reviews:
        if (entry["state"] == "CHANGES_REQUESTED" and entry.get("body", "").strip()
                and entry["user"]["login"] in reviewers):
            result.append(item("review", entry, entry.get("commit_id")))
    for entry in inline:
        if entry["pull_request_review_id"] not in published or entry["user"]["login"] not in reviewers:
            continue
        root = roots.get(entry.get("in_reply_to_id", entry["id"]))
        if root is None:
            raise ForgeError("Published review comment has no corresponding thread")
        if not root["isResolved"] and not root["isOutdated"] and entry.get("body", "").strip():
            result.append(item("inline", entry, entry.get("commit_id")))
    for entry in comments:
        body = entry.get("body", "")
        if entry["user"]["login"] not in reviewers:
            continue
        marker = re.search(r"<!-- (?:pr-queue:|pr-task:feedback )([0-9a-f-]{36}) -->", body)
        source = re.search(r"PR head: `([0-9a-f]{40})`", body)
        if marker and source and source[1] == pr["head"]["sha"]:
            result.append(item("feedback", entry, source[1]))
    return result


def snapshot(state):
    deadline = time.monotonic() + 90
    endpoint = f"repos/{state['repository']}"
    number = state["pr"]
    pull = f"{endpoint}/pulls/{number}"
    pr = api(pull, deadline=deadline)
    if (pr["base"]["repo"]["full_name"] != state["repository"]
            or pr["head"]["repo"] is None
            or pr["head"]["repo"]["full_name"] != state["repository"]
            or pr["head"]["ref"] != state["branch"] or pr["base"]["ref"] != state["base"]):
        raise ForgeError("PR repository or branch differs from the registered task")
    result = {"head": pr["head"]["sha"], "base": pr["base"]["sha"], "url": pr["html_url"],
              "state": "merged" if pr["merged"] else pr["state"], "items": [], "ci": "pending",
              "merge_commit": pr.get("merge_commit_sha"), "draft": pr["draft"]}
    if result["state"] != "open":
        return result
    reviews = pages(f"{pull}/reviews", deadline)
    comments = pages(f"{endpoint}/issues/{number}/comments", deadline)
    inline = pages(f"{pull}/comments", deadline)
    roots = thread_roots(state["repository"], number, deadline) if inline else {}
    result["items"] = feedback(pr, reviews, comments, inline, roots, state["reviewers"])
    checks = pages(f"{endpoint}/commits/{result['head']}/check-runs?filter=latest",
                   deadline, "check_runs")
    statuses = pages(f"{endpoint}/commits/{result['head']}/statuses", deadline)
    latest = {}
    for entry in statuses:
        latest.setdefault(entry["context"], entry)
    for entry in checks:
        if entry["head_sha"] != result["head"]:
            raise ForgeError("Check run belongs to a different commit")
        if entry["status"] == "completed" and entry["conclusion"] in {
            "failure", "timed_out", "cancelled", "action_required", "startup_failure", "stale",
        }:
            result["items"].append({
                "kind": "check", "id": str(entry["id"]), "head": result["head"],
                "key": digest(["check", result["head"], entry["id"], entry.get("completed_at")]),
                "body": f"{entry['name']}: {entry['conclusion']}", "url": entry["html_url"],
            })
    for entry in latest.values():
        if entry["state"] in {"failure", "error"}:
            result["items"].append({
                "kind": "check", "id": str(entry["id"]), "head": result["head"],
                "key": digest(["status", result["head"], entry["id"]]),
                "body": f"{entry['context']}: {entry['state']}", "url": entry.get("target_url"),
            })
    conclusions = {entry["name"]: entry["conclusion"] if entry["status"] == "completed" else "pending"
                   for entry in checks}
    conclusions.update({name: entry["state"] for name, entry in latest.items()})
    if any(entry["kind"] == "check" for entry in result["items"]):
        result["ci"] = "failed"
    elif conclusions and all(conclusions.get(name) == "success" for name in state["checks"]):
        result["ci"] = "passed" if all(value == "success" for value in conclusions.values()) else "pending"
    after = api(pull, deadline=deadline)
    if (after["head"]["sha"], after["base"]["sha"], after["state"], after["merged"]) != (
            result["head"], result["base"], pr["state"], pr["merged"]):
        raise ForgeError("PR changed while collecting its snapshot; retry without consuming feedback")
    return result
