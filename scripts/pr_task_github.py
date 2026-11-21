import json
import hashlib
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
            headers.get("x-ratelimit-remaining") == "0" or "retry-after" in headers
            or "rate limit" in message.lower())
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
    status = int(header.splitlines()[0].split()[1])
    return status, headers, {} if status == 204 and not body.strip() else json.loads(body)


def api(endpoint, *arguments, deadline=None):
    timeout = 60 if deadline is None else max(0.1, min(60, deadline - time.monotonic()))
    if deadline is not None and time.monotonic() >= deadline:
        raise ForgeError("GitHub snapshot exceeded its time budget")
    try:
        raw = github("api", "--hostname", "github.com", "--include", endpoint, *arguments, timeout=timeout)
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
    resolved = set()
    for entry in comments:
