import argparse
from datetime import datetime, timezone
import fcntl
import json
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.promote_candidate import ROOT, git, github, identity, promote, promote_context
from scripts.validate_context import context_only, require_remote_target, require_target
from scripts import pr_queue_gates


def now():
    return datetime.now(timezone.utc).isoformat()


def save(path, record):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def require_checkout(state):
    if (Path.cwd().resolve() != ROOT or identity() != state["source"]
            or git("status", "--porcelain", "--untracked-files=all")
            or git("branch", "--show-current") != state["branch"]):
        raise ValueError("Continue only in the exact clean candidate checkout and branch")


def find_run(repository, branch, candidate, pinned=None):
    if pinned is not None:
        run = json.loads(github("api", f"repos/{repository}/actions/runs/{pinned}", timeout=60))
        if (run["id"] != pinned or run["head_sha"] != candidate or run["head_branch"] != branch
                or run["repository"]["full_name"] != repository
                or run["path"] != ".github/workflows/plugin-candidate.yml"
                or run["event"] not in ("push", "workflow_dispatch")):
            raise ValueError("Recorded workflow no longer identifies this exact candidate")
        return run
    runs = json.loads(github("api", f"repos/{repository}/actions/workflows/"
                            f"plugin-candidate.yml/runs?branch={quote(branch, safe='')}&per_page=100",
                            timeout=60))
    matches = [run for run in runs["workflow_runs"]
               if run["head_sha"] == candidate and run["head_branch"] == branch
               and run["event"] in ("push", "workflow_dispatch")]
    return max(matches, key=lambda run: run["id"]) if matches else None


def waiting(state, path, kind):
    state.update(merge_status="waiting-ci", waiting_on=kind, observed_at=now())
    state.pop("last_error", None)
    save(path, state)
    return state


def failed_run(state, run):
    state["merge_status"] = "infrastructure-blocked"
    state.pop("failed_jobs", None)
    if run["conclusion"] == "failure":
        pages = json.loads(github("api", "--paginate", "--slurp",
                                 f"repos/{state['repository']}/actions/runs/{run['id']}"
                                 f"/attempts/{run['run_attempt']}/jobs?per_page=100", timeout=60))
        state["failed_jobs"] = [job for page in pages for job in page["jobs"]
                                if job["conclusion"] != "success"]
        if any(step.get("conclusion") == "failure" and "scripts/validate" in step["name"]
               for job in state["failed_jobs"] if job["name"].startswith("Candidate (")
               for step in job.get("steps", [])):
            state["merge_status"] = "needs-author"
    raise ValueError(f"Candidate run {run['id']} finished: {run['conclusion']}")


def continue_candidate(state, directory, timeout):
    state_path = directory / "integration.json"
    require_checkout(state)
    candidate, base = state["source"]["commit"], state["base"]
    reference = state.get("reference", "refs/heads/main")
    require_target(reference, base, state["branch"], candidate, git_command=git)
    require_remote_target(reference, (base, candidate), git_command=git)
    if state.get("completed_at"):
        state["merge_status"] = "merged"
        return state
    subprocess.run(["git", "merge-base", "--is-ancestor", base, candidate], cwd=ROOT, check=True)
    if not state.get("pushed_at"):
        subprocess.run(["git", "push", "origin", f"{candidate}:refs/heads/{state['branch']}"],
                       cwd=ROOT, check=True, timeout=120)
        state["pushed_at"] = now()
        save(state_path, state)
    deadline = time.monotonic() + timeout
    while True:
        run = find_run(state["repository"], state["branch"], candidate, state.get("run"))
        if run:
            state["run"] = run["id"]
            state["run_attempt"] = run["run_attempt"]
            state["workflow"] = {key: run.get(key) for key in
                                 ("id", "run_attempt", "status", "conclusion", "html_url")}
            save(state_path, state)
            if run["status"] == "completed":
                if run["conclusion"] != "success":
                    failed_run(state, run)
                break
        if time.monotonic() >= deadline:
            return waiting(state, state_path, "candidate")
        time.sleep(min(10, max(0, deadline - time.monotonic())))
    require_checkout(state)
    if state.get("pr_gates"):
        while True:
            state["pr_checks"] = pr_queue_gates.verify(state["pr_gates"])
            save(state_path, state)
            if state["pr_checks"] is not None:
                break
            if time.monotonic() >= deadline:
                return waiting(state, state_path, "pr")
            time.sleep(min(10, max(0, deadline - time.monotonic())))
    receipt = directory / "promotion.json"
    if receipt.exists():
        saved = json.loads(receipt.read_text())
        if (saved["source"] != state["source"] or saved["base"] != base
                or saved["run"] != run["id"] or saved["repository"] != state["repository"]
                or saved.get("reference", "refs/heads/main") != reference):
            raise ValueError("Existing promotion receipt belongs to another integration")
        require_remote_target(reference, (candidate,), git_command=git)
    else:
        if state.get("records_only"):
            promote_context(candidate, base, run["id"], state["repository"], receipt)
        elif state.get("pr_gates"):
            promote(candidate, base, run["id"], state["repository"], receipt, reference,
                    pr_gates=state["pr_gates"])
        else:
            promote(candidate, base, run["id"], state["repository"], receipt, reference)
    state["completed_at"] = now()
    state["merge_status"] = "merged"
    state.pop("waiting_on", None)
    promoted = json.loads(receipt.read_text())
    state["completed_at"] = promoted.get("promoted_at", state["completed_at"])
    state["release_status"] = promoted.get("release_status", "unchanged")
    state["release"] = promoted.get("release")
    state["seconds"] = (datetime.fromisoformat(state["completed_at"])
                        - datetime.fromisoformat(state["started_at"])).total_seconds()
    state.pop("last_error", None)
    save(state_path, state)
    return state


def start(arguments):
    directory = arguments.output.resolve()
    if directory.is_relative_to(ROOT):
        raise ValueError("Integration records must remain outside the source")
    path = directory / "integration.json"
    reference = getattr(arguments, "reference", "refs/heads/main")
    if arguments.resume:
        state = json.loads(path.read_text())
        if state.get("reference", "refs/heads/main") != reference:
            raise ValueError("Resume must explicitly select the recorded integration target")
    else:
        if not arguments.base or not arguments.repository or directory.exists():
            raise ValueError("A new integration needs fixed base, repository and a new output directory")
        branch = git("branch", "--show-current")
        if not branch.startswith("codex/integration-"):
            raise ValueError("Use an isolated codex/integration-* candidate branch")
        started = datetime.fromisoformat(arguments.started_at or now())
        if started.tzinfo is None or started > datetime.now(timezone.utc):
            raise ValueError("Start time must identify a timezone-aware past instant")
        state = {
            "schema_version": 1, "source": identity(), "branch": branch,
            "base": git("rev-parse", f"{arguments.base}^{{commit}}"),
            "reference": reference,
            "repository": arguments.repository, "started_at": started.isoformat(),
        }
        require_checkout(state)
        state["records_only"] = reference == "refs/heads/main" and context_only(state["base"])
        if reference == "refs/heads/main" and not state["records_only"]:
            from scripts.main_release_evidence import require_version
            require_version(arguments.repository, state["base"])
        if getattr(arguments, "pr_gates", None):
            state["pr_gates"] = pr_queue_gates.from_queue(
                arguments.pr_gates, ROOT, arguments.repository, state["source"]["commit"])
        directory.mkdir(parents=True)
        save(path, state)
    invocation = {"started_at": now(), "status": "running"}
    for previous in state.get("invocations", []):
        if previous["status"] == "running":
            previous.update(status="interrupted", interruption_observed_at=now())
    state.setdefault("invocations", []).append(invocation)
    state["merge_status"] = "checking"
    save(path, state)
    try:
        if arguments.resume and getattr(arguments, "pr_gates", None):
            gates = pr_queue_gates.from_queue(
                arguments.pr_gates, ROOT, state["repository"], state["source"]["commit"])
            if state.get("pr_gates", gates) != gates:
                raise ValueError("Resumed PR gates differ from the frozen source heads")
            if state.get("completed_at") and not state.get("pr_gates"):
                raise ValueError("An old completed integration cannot acquire PR gates retroactively")
            state["pr_gates"] = gates
        result = continue_candidate(state, directory, arguments.timeout)
    except Exception as error:
        if state["merge_status"] == "checking":
            state["merge_status"] = getattr(error, "result", (
                "needs-user" if isinstance(error, ValueError) else "infrastructure-blocked"))
        state["last_error"] = {"at": now(), "type": type(error).__name__, "detail": str(error)}
        state.setdefault("errors", []).append(state["last_error"])
        raise
    finally:
        invocation.update(finished_at=now(), status=state["merge_status"])
        invocation["workflow"] = state.get("workflow")
        invocation["seconds"] = (datetime.fromisoformat(invocation["finished_at"])
                                 - datetime.fromisoformat(invocation["started_at"])).total_seconds()
        state["seconds"] = (datetime.fromisoformat(state.get("completed_at") or now())
                            - datetime.fromisoformat(state["started_at"])).total_seconds()
        save(path, state)
        if state.get("workflow", {}).get("conclusion") not in (None, "success"):
            failure = directory / f"failed-run-{state['run']}-{state['run_attempt']}.json"
            if not failure.exists():
                save(failure, state)
    print(json.dumps({"source": result["source"]["commit"], "run": result.get("run"),
                      "merge_status": result.get("merge_status"),
                      "release_status": result.get("release_status"),
                      "seconds": result["seconds"], "invocation": invocation, "receipt": str(path)}))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base")
    parser.add_argument("--reference", default="refs/heads/main")
    parser.add_argument("--repository")
    parser.add_argument("--started-at")
    parser.add_argument("--pr-gates", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--timeout", type=int, default=0)
    arguments = parser.parse_args()
    if arguments.timeout < 0:
        parser.error("--timeout must be nonnegative")
    try:
        output = arguments.output.resolve()
        if output.is_relative_to(ROOT):
            raise ValueError("Integration records must remain outside the source")
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.with_name(f".{output.name}.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            start(arguments)
    except BlockingIOError:
        print("Integration is busy; resume this same output directory", file=sys.stderr)
        return 75
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
