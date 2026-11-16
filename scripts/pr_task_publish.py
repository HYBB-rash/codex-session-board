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
        state.update(pr=remote_pr["number"], phase="finalizing", cleanup_pending=True)
        persist(state)
        return
    if remote_pr and state["pr"] not in (None, remote_pr["number"]):
        raise ValueError("Task is already bound to a different PR")
    if remote_pr and remote_pr["head"]["sha"] not in {
            head, state.get("published", {}).get("head"),
            state.get("last_snapshot", {}).get("head")} and not own_publish_intent(state, remote_pr["head"]["sha"], head):
        raise ValueError("Another writer changed the PR; inspect before publishing")
    if not remote_pr and (not title or not body):
        raise ValueError("The first publication needs --title and --body-file")
    if state.get("published", {}).get("head") == head:
        output = Path(state["published"]["validation"])
        validation(state, output, head)
    else:
        output = Path(state["directory"]) / f"validation-{head[:12]}-{uuid.uuid4().hex[:8]}"
        record(state, "validation_started", head=head, output=str(output))
        subprocess.run([str(ROOT / "scripts/validate"), "--scope", "pr", "--stage", "submit",
                        "--project", state["project"],
                        "--base", state["base_commit"], "--expect-head", head, "--output", str(output)],
                       cwd=ROOT, check=True, timeout=1900)
        validation(state, output, head)
    if checkout(state, clean=True) != head:
        raise ValueError("Source changed during validation")
    before_push = published_pr(state)
    if before_push and (before_push["state"] != "open" or remote_pr is None
                        or before_push["head"]["sha"] != remote_pr["head"]["sha"]):
        raise ValueError("PR changed during validation; nothing pushed")
    state["publish_intent"] = {
        "head": head, "validation": str(output), "at": now(),
        "validation_sha256": hashlib.sha256((output / "VALIDATION.json").read_bytes()).hexdigest(),
    }
    persist(state)
    subprocess.run(["git", "push", state["remote"], f"HEAD:refs/heads/{state['branch']}"],
                   cwd=state["project"], check=True, timeout=120)
    remote_pr = published_pr(state)
    if not remote_pr:
        remote_pr = forge.api(f"repos/{state['repository']}/pulls", "--method", "POST",
                              "-f", f"title={title}", "-f", f"body={body}",
                              "-f", f"head={state['branch']}", "-f", f"base={state['base']}",
                              "-F", "draft=true")
    observed = forge.api(f"repos/{state['repository']}/pulls/{remote_pr['number']}")
    if observed["head"]["sha"] != head:
        raise ValueError("Published PR does not contain the validated head")
    state.update(pr=observed["number"], phase="waiting-ci",
                 published={"head": head, "validation": str(output), "at": now()}, next_poll_at=0)
    persist(state)
    record(state, "published", pr=state["pr"], head=head, draft=observed["draft"])


def ready(state, snapshot):
    publication = state.get("published")
    if (not publication or publication["head"] != snapshot["head"]
            or snapshot["ci"] != "passed" or state.get("ready_head") == snapshot["head"]):
        return
    validation(state, Path(publication["validation"]), snapshot["head"])
    current = forge.api(f"repos/{state['repository']}/pulls/{state['pr']}")
    if current["state"] != "open" or current["head"]["sha"] != snapshot["head"]:
        raise ValueError("PR changed before announcing readiness")
    if current["draft"]:
        forge.api("graphql", "-f", "query=mutation($id:ID!){markPullRequestReadyForReview(input:{pullRequestId:$id}){pullRequest{id}}}",
                  "-f", f"id={current['node_id']}")
    marker = f"<!-- pr-task:ready {state['id']} {snapshot['head']} -->"
    comment_once(state["repository"], state["pr"], marker,
                 f"作者本机交付检查及准确头 GitHub CI 已通过。\nPR head: `{snapshot['head']}`\n"
                 "请独立审阅后纳入组合候选；此记录不表示已经合并、部署或完成目标环境验收。")
    labels(state["repository"], state["pr"], True)
    state["ready_head"] = snapshot["head"]
    persist(state)


def feedback(repository, number, expected_head, request_id, body):
    uuid.UUID(request_id)
    pr = forge.api(f"repos/{repository}/pulls/{number}")
    if pr["state"] != "open" or pr["head"]["sha"] != expected_head:
        raise ValueError("Feedback must name the current open PR head")
    marker = f"<!-- pr-task:feedback {request_id} -->"
    url = comment_once(repository, number, marker,
                       f"PR head: `{expected_head}`\n\n{body}")
    labels(repository, number, False)
    return {"feedback_id": request_id, "url": url, "head": expected_head}
