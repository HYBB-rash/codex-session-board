import json
from pathlib import Path
import re
import time

from scripts import pr_task_github as forge


def from_queue(path, project, repository, candidate):
    state = json.loads(Path(path).read_text())
    if (state.get("rehearsal") or state["repository"] != repository
            or Path(state["project"]).resolve() != project.resolve()
            or state.get("frozen_candidate", {}).get("candidate") != candidate):
        raise ValueError("PR gates must identify this exact real queue and frozen candidate")
    items = []
    for item in state["items"]:
        if item["status"] not in {"accepted", "merged"}:
            continue
        if (type(item["number"]) is not int or item["number"] <= 0
                or not re.fullmatch("[0-9a-f]{40}", item["head"])):
            raise ValueError("PR gates require distinct PR numbers and exact source heads")
        checks = item.get("checks", ["changes"])
        reviewers = item.get("reviewers", [repository.split("/")[0]])
        if (not isinstance(checks, list) or "changes" not in checks or not isinstance(reviewers, list)
                or not reviewers or not all(isinstance(value, str) and value for value in checks + reviewers)):
            raise ValueError("PR gates require explicit checks and trusted reviewers")
        items.append({"number": item["number"], "head": item["head"],
                      "checks": checks, "reviewers": reviewers})
    if not items or len({item["number"] for item in items}) != len(items):
        raise ValueError("PR gates need distinct accepted PRs")
    return {"repository": repository, "items": items}


def workflow(repository, number, head):
    deadline = time.monotonic() + 90
    runs = forge.pages(
        f"repos/{repository}/actions/workflows/pr-checks.yml/runs?event=pull_request&head_sha={head}",
        deadline, "workflow_runs")
    matches = [run for run in runs if run["head_sha"] == head and run["event"] == "pull_request"
               and run["path"] == ".github/workflows/pr-checks.yml"
               and run["repository"]["full_name"] == repository
               and any(pr["number"] == number and pr["head"]["sha"] == head for pr in run["pull_requests"])]
    if not matches:
        return None
    run = max(matches, key=lambda item: item["id"])
    if run["status"] != "completed":
        return None
    if run["conclusion"] != "success":
        raise ValueError(f"PR #{number} workflow did not pass")
    jobs = forge.pages(f"repos/{repository}/actions/runs/{run['id']}/jobs?filter=latest", deadline, "jobs")
    if (not any(job["name"] == "changes" for job in jobs)
            or any(job["head_sha"] != head or job["status"] != "completed"
                   or job["conclusion"] != "success" for job in jobs)):
        raise ValueError(f"PR #{number} requires its actual successful changes job")
    return {"run": run["id"], "attempt": run["run_attempt"], "head": head}


def verify(gates):
    results = []
    for item in gates["items"]:
        repository, number = gates["repository"], item["number"]
        pr = forge.api(f"repos/{repository}/pulls/{number}")
        if pr["head"]["sha"] != item["head"] or pr["base"]["ref"] != "main":
            raise ValueError(f"PR #{number} changed source or target; requeue its new head")
        snapshot = forge.snapshot({
            "repository": repository, "pr": number, "branch": pr["head"]["ref"],
            "base": "main", "checks": item["checks"], "reviewers": item["reviewers"],
        })
        if snapshot["head"] != item["head"] or snapshot["state"] not in {"open", "merged"}:
            raise ValueError(f"PR #{number} changed or closed without merging")
        if snapshot["state"] == "merged":
            snapshot.update(forge.check_state(repository, item["head"], item["checks"],
                                             time.monotonic() + 90))
        if snapshot["ci"] == "failed" or snapshot["items"]:
            raise ValueError(f"PR #{number} has failed checks or unresolved review feedback")
        proof = workflow(repository, number, item["head"]) if snapshot["ci"] == "passed" else None
        after = forge.api(f"repos/{repository}/pulls/{number}")
        if (after["head"]["sha"] != item["head"] or after["base"]["ref"] != "main"
                or after["head"]["ref"] != pr["head"]["ref"]
                or after["base"]["sha"] != snapshot.get("base", after["base"]["sha"])
                or ("merged" if after["merged"] else after["state"]) != snapshot["state"]):
            raise ValueError(f"PR #{number} changed while proving its workflow; requeue")
        results.append({"number": number, **snapshot, "workflow": proof})
    return results if all(result["ci"] == "passed" and result["workflow"] for result in results) else None
