import json
import hashlib
from pathlib import Path
import subprocess
import time
import uuid

from scripts import pr_task_github as forge
from scripts.pr_queue import verify_pr_validation
from scripts.pr_task_state import ROOT, checkout, digest, git, now, persist, record, save
from scripts.validate_candidate import identity


def comment_once(repository, number, marker, body):
    endpoint = f"repos/{repository}/issues/{number}/comments"
    comments = forge.pages(endpoint, time.monotonic() + 60)
    matches = [item for item in comments if marker in item["body"]]
    if len(matches) > 1:
        raise ValueError("Feedback marker appears more than once")
    if matches:
        if matches[0]["body"] != f"{marker}\n{body}":
            raise ValueError("An existing comment marker has different content")
        return matches[0]["html_url"]
    created = forge.api(endpoint, "--method", "POST", "-f", f"body={marker}\n{body}")
    observed = forge.api(f"repos/{repository}/issues/comments/{created['id']}")
    if observed["body"] != f"{marker}\n{body}":
        raise RuntimeError("GitHub did not confirm the exact published comment")
    return observed["html_url"]


def ensure_labels(repository):
    endpoint = f"repos/{repository}"
    available = {item["name"] for item in forge.pages(f"{endpoint}/labels", time.monotonic() + 60)}
    for name, color in (("ready-for-review", "0e8a16"), ("needs-fix", "d73a4a")):
        if name not in available:
            forge.api(f"{endpoint}/labels", "--method", "POST", "-f", f"name={name}", "-f", f"color={color}")


def labels(repository, number, ready):
    ensure_labels(repository)
    endpoint = f"repos/{repository}"
    issue = forge.api(f"{endpoint}/issues/{number}")
    assigned = {item["name"] for item in issue["labels"]}
    wanted, unwanted = ("ready-for-review", "needs-fix") if ready else ("needs-fix", "ready-for-review")
    if wanted not in assigned:
        forge.api(f"{endpoint}/issues/{number}/labels", "--method", "POST", "-f", f"labels[]={wanted}")
    if unwanted in assigned:
        forge.api(f"{endpoint}/issues/{number}/labels/{unwanted}", "--method", "DELETE")


def published_pr(state):
    owner = state["repository"].split("/")[0]
    from urllib.parse import urlencode
    query = urlencode({"state": "all", "head": f"{owner}:{state['branch']}", "base": state["base"]})
    entries = forge.pages(f"repos/{state['repository']}/pulls?{query}", time.monotonic() + 60)
    exact = [entry for entry in entries if entry["head"]["repo"]
             and entry["head"]["repo"]["full_name"] == state["repository"]
             and entry["head"]["ref"] == state["branch"]]
    if len(exact) > 1:
        raise ValueError("This branch has multiple historical PRs; choose a new task branch")
    return exact[0] if exact else None


def validation(state, report_path, head):
    report = json.loads((report_path / "VALIDATION.json").read_text())
    verify_pr_validation({"head": head, "base": state["base_commit"],
                          "validation": str(report_path)}, state["checker"])
    if report["project"] != state["project"]:
        raise ValueError("Validation belongs to a different worktree")
    return report


def own_publish_intent(state, remote_head, head):
    intent = state.get("publish_intent", {})
    if remote_head != intent.get("head"):
        return False
    path = Path(intent["validation"]) / "VALIDATION.json"
    if hashlib.sha256(path.read_bytes()).hexdigest() != intent.get("validation_sha256"):
        raise ValueError("Uncertain publication report changed")
    report = json.loads(path.read_text())
    if report["source"]["commit"] != remote_head or report["project"] != state["project"] or not report["passed"]:
        raise ValueError("Uncertain publication report does not identify this task")
    subprocess.run(["git", "merge-base", "--is-ancestor", remote_head, head],
                   cwd=state["project"], check=True, timeout=30)
    return True


def submit(state, title=None, body=None):
    if state["phase"] in {"complete", "finalizing", "paused", "needs-human"}:
        raise ValueError("Task is not accepting publication")
    head = checkout(state, clean=True)
    remote_pr = published_pr(state)
    if remote_pr and remote_pr["state"] != "open":
        if state["pr"] != remote_pr["number"] and not own_publish_intent(state, remote_pr["head"]["sha"], head):
            raise ValueError("Historical closed PR does not belong to this task; choose a new branch")
