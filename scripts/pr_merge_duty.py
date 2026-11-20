import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import pr_task_github as forge
from scripts.pr_task_state import digest, git, load, locked, now, private_directory, save
from scripts.validate_candidate import identity


SHA = re.compile(r"[0-9a-f]{40}")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
REMOTE = re.compile(r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)(.+?)(?:\.git)?")
PRODUCTION = "hybb-rash/codex-session-board"


def read(path):
    return json.loads(Path(path).read_text())


def verify_origin(project, repository):
    for options in (("--all",), ("--push", "--all")):
        url = git(project, "remote", "get-url", *options, "origin")
        match = REMOTE.fullmatch(url)
        if "\n" in url or not match or match[1].lower() != repository.lower():
            raise ValueError("Candidate fetch and push origin must identify the configured repository")


def configuration(path):
    config = read(path)
    repository = config["repository"]
    if not REPOSITORY.fullmatch(repository):
        raise ValueError("Configure an explicit GitHub repository")
    if config.get("mode") != "isolated" or repository.lower() == PRODUCTION:
        raise ValueError("The duty prototype is for isolated repositories. Use pr-queue for manual merges.")
    checker = Path(config["checker"]).resolve(strict=True)
    if not SHA.fullmatch(config["checker_head"]) or git(checker, "rev-parse", "HEAD") != config["checker_head"]:
        raise ValueError("The configured checker must be at the frozen commit")
    if git(checker, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("The frozen checker must remain clean")
    directory = Path(config["state"]).resolve()
    if not Path(config["state"]).is_absolute() or directory.is_relative_to(checker) or directory.is_relative_to(ROOT):
        raise ValueError("Use an absolute, private state directory outside source")
    assignments = config.get("assignments", [])
    if not isinstance(assignments, list) or len(assignments) > 1:
        raise ValueError("The trial accepts at most one explicitly assigned PR")
    for assignment in assignments:
        if type(assignment["pr"]) is not int or assignment["pr"] < 1 or not SHA.fullmatch(assignment["head"]):
            raise ValueError("An assignment must identify a positive PR number and its full authorized head")
        candidate = Path(assignment["candidate"]).resolve(strict=True)
        if candidate in (checker, ROOT) or directory.is_relative_to(candidate):
            raise ValueError("Use a dedicated candidate worktree and keep runtime state outside it")
        if git(candidate, "rev-parse", "--show-toplevel") != str(candidate):
            raise ValueError("Candidate must identify the worktree root")
        if not git(candidate, "branch", "--show-current").startswith("codex/integration-"):
            raise ValueError("Candidate must use a dedicated codex/integration- branch")
        verify_origin(candidate, repository)
    return config, checker, private_directory(directory)


def observation(config, assignment, checker):
    number, head = assignment["pr"], assignment["head"]
    repository = config["repository"]
    pr = forge.api(f"repos/{repository}/pulls/{number}")
    fields = {"pr": number, "head": head, "url": pr["html_url"], "observed_head": pr["head"]["sha"]}
    if (pr["number"] != number or pr["base"]["repo"]["full_name"] != repository
            or pr["base"]["ref"] != "main" or not pr["head"]["repo"]
            or pr["head"]["repo"]["full_name"] != repository):
        return {**fields, "result": "needs-user", "detail": "PR repository or target differs from its assignment"}
    if fields["observed_head"] != head:
        return {**fields, "result": "needs-user", "detail": "PR head changed; renew the assignment and evidence"}
    if pr["state"] != "open":
        return {**fields, "result": "closed", "github_merged": pr["merged"]}
    try:
        task = load(Path(assignment["author_task"]).resolve(strict=True))
    except FileNotFoundError:
        return {**fields, "result": "needs-evidence", "detail": "Author task is missing"}
    publication = task.get("published") or {}
    if (task["repository"], task["pr"], task["base"], task["branch"], publication.get("head"),
            task["checker"], Path(task["checker_project"]).resolve()) != (
            repository, number, "main", pr["head"]["ref"], head, identity(checker), checker):
        return {**fields, "result": "needs-evidence", "detail": "Author publication or checker does not match PR"}
    if task.get("archive"):
        return {**fields, "result": "needs-user", "detail": "Author task can archive before the result is delivered"}
    if pr["draft"]:
        checks = forge.check_state(repository, head, task["checks"], time.monotonic() + 90)
        return {**fields, "result": "waiting-ci" if checks["ci"] == "pending" else "waiting-author",
                "detail": "PR is a draft; the author must complete readiness",
                "draft": True, "ci": checks["ci"], "checks": checks["items"]}
    review_path = Path(assignment["review"]).resolve()
    try:
        review = read(review_path)
    except FileNotFoundError:
        return {**fields, "result": "waiting-review", "detail": "Independent review is missing"}
    if (review.get("head") != head or review.get("passed") is not True
            or not review.get("reviewer") or review["reviewer"] == task["route"]["thread_id"]):
        return {**fields, "result": "waiting-review", "detail": "Current head requires an independent review"}
    snapshot = forge.snapshot(task)
    if snapshot["head"] != head or snapshot["state"] != "open" or snapshot["draft"]:
        return {**fields, "result": "needs-user", "detail": "PR changed during admission"}
    if snapshot["items"] or snapshot["ci"] == "failed":
        return {**fields, "result": "needs-author", "feedback": snapshot["items"]}
    if snapshot["ci"] != "passed":
        return {**fields, "result": "waiting-ci"}
    from scripts.pr_queue_gates import workflow
    try:
        proof = workflow(repository, number, head)
    except ValueError as error:
        return {**fields, "result": getattr(error, "result", "needs-evidence"), "detail": str(error)}
    if proof is None:
        return {**fields, "result": "waiting-ci"}
    item = {
        "number": number, "head": head, "base": task["base_commit"], "dependencies": [],
        "review": str(review_path), "validation": publication["validation"],
        "route": task["route"], "author_task_state": task["directory"],
        "reviewers": task["reviewers"], "checks": task["checks"],
    }
    from scripts.pr_queue import verify_pr_validation
    try:
        verify_pr_validation(item, task["checker"])
    except (ValueError, FileNotFoundError) as error:
        return {**fields, "result": "needs-evidence", "detail": str(error)}
    return {**fields, "result": "ready", "workflow": proof, "item": item}


def queue_command(checker, command, directory, *arguments):
    process = subprocess.run([str(checker / "scripts/pr-queue"), command, "--state", str(directory),
                              *map(str, arguments)], text=True, stdout=subprocess.PIPE, timeout=720)
    if process.returncode == 75:
        raise BlockingIOError("Queue is busy")
    try:
        state = json.loads(process.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"pr-queue {command}: {process.stdout.strip()}") from error
    if command == "status" and process.returncode:
        raise ValueError(f"pr-queue status: {process.stdout.strip()}")
    return state


def verify_queue(state, config, assignment, checker, candidate):
    if (state["repository"], Path(state["project"]).resolve(), state["checker"], state.get("feedback_mode"),
            state.get("rehearsal", False), state.get("reference", "refs/heads/main")) != (
            config["repository"], candidate, identity(checker), "github", False, "refs/heads/main"):
        raise ValueError("Existing queue has a different repository, candidate, or checker")
    if [(item["number"], item["head"]) for item in state["items"]] != [(assignment["pr"], assignment["head"])]:
        raise ValueError("Existing queue does not bind this single authorized PR head")


def advance(config, checker, directory, assignment, observed):
    repository = config["repository"]
    candidate = Path(assignment["candidate"]).resolve(strict=True)
    identifier = digest([repository.lower(), assignment["pr"], assignment["head"]])
    queue = directory / f"queue-{identifier}"
    if not queue.exists():
        from scripts.pr_queue_registry import locate
        try:
            queue = locate(repository, assignment["pr"], checker)
        except FileNotFoundError:
            index = Path(git(checker, "rev-parse", "--path-format=absolute", "--git-common-dir"))
            if (index / "pr-queues" / f"{digest([repository.lower(), assignment['pr']])}.json").exists():
                raise ValueError("A queue is registered but its original records are missing")
    if not queue.exists():
        if observed["result"] != "ready":
            return observed
        base = forge.api(f"repos/{repository}/git/ref/heads/main")["object"]["sha"]
        if not SHA.fullmatch(base) or git(candidate, "rev-parse", "HEAD") != base:
            raise ValueError("Candidate must start from the current remote main")
        git(candidate, "fetch", "--no-tags", "origin", f"refs/pull/{assignment['pr']}/head")
        if git(candidate, "rev-parse", "FETCH_HEAD") != assignment["head"]:
            raise ValueError("PR head changed while fetching the candidate input")
        manifest = directory / f"manifest-{identifier}.json"
        save(manifest, {"repository": repository, "feedback_mode": "github", "project": str(candidate),
                        "base": base, "items": [observed["item"]]})
        state = queue_command(checker, "init", queue, "--manifest", manifest)
    else:
        state = queue_command(checker, "status", queue)
    verify_queue(state, config, assignment, checker, candidate)
    item = state["items"][0]
    if observed["result"] == "closed" and item["status"] == "merged":
        state = queue_command(checker, "status", queue, "--refresh")
    elif observed["result"] == "closed" and observed["github_merged"] and state.get("frozen_candidate"):
        state = queue_command(checker, "finish", queue)
    elif observed["result"] == "closed" or (
            state.get("frozen_candidate") and not observed.get("draft")
            and observed["result"] in {"ready", "waiting-ci"}):
        state = queue_command(checker, "resume", queue)
    elif item.get("feedback") and observed["result"] in {"ready", "needs-author"}:
        if not item["feedback"].get("comment"):
            state = queue_command(checker, "notify", queue)
    elif observed["result"] == "ready":
        check = queue_command(checker, "status", queue, "--refresh")["observation"]
        if check["main"] != state["base"] or check["result"] != "ready":
            return {**observed, "result": "needs-user", "detail": "Remote state changed before mechanical combination"}
        if item["status"] == "submitted":
            state = queue_command(checker, "attempt", queue)
        if state["items"][0]["status"] == "accepted":
            from scripts import pr_queue
            prefix = state["prefix"]
            head = git(candidate, "rev-parse", "HEAD")
            if head == prefix:
                pr_queue.clean(candidate, prefix)
                log_path = directory / f"freeze-{identifier}.log"
                with log_path.open("a") as log:
                    subprocess.run([str(candidate / "scripts/check"), "--prepare-release"], cwd=candidate,
                                   stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
                    git(candidate, "add", "--", "Cargo.toml", "Cargo.lock", "project-rules.json",
                        "plugin.json", ".codex-plugin/plugin.json")
                    subprocess.run([str(candidate / "scripts/check"), "--pre-commit"], cwd=candidate,
                                   stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
                pr_queue.commit(candidate, "release: freeze PR merge duty candidate")
            pr_queue.validate_freeze(candidate, prefix, git(candidate, "rev-parse", "HEAD"))
            state = queue_command(checker, "finish", queue)
    handoff = state.get("handoff") or {}
    if (observed["result"] == "closed" and handoff.get("result") == "merged") or state.get("batch_merged"):
        if not state.get("batch_merged") or not state.get("frozen_candidate"):
            return {**observed, "result": "needs-user", "detail": "PR was merged outside this duty candidate"}
        verified = queue_command(checker, "status", queue, "--refresh")
        if verified["observation"]["items"][0]["result"] != "merged":
            return {**observed, "result": "waiting-pr-terminal", "queue": str(queue),
                    "candidate": state["frozen_candidate"], "integration": state["integration"]}
        try:
            observed["release_status"] = queue_command(checker, "status", queue, "--release").get(
                "release_status", "pending")
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            observed.update(release_status="unknown", release_status_error=str(error))
    result = observed["result"]
    if result in {"closed", "ready"} or result == "waiting-ci" and state.get("frozen_candidate"):
        result = handoff.get("result", result)
    return {key: value for key, value in observed.items() if key != "item"} | {
        "result": result, "queue": str(queue),
        "candidate": state.get("frozen_candidate"), "integration": state.get("integration"),
        "feedback": state["items"][0].get("feedback"), "handoff": handoff,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("scan", "tick", "pause", "resume", "status"))
    parser.add_argument("--config", required=True, type=Path)
    arguments = parser.parse_args()
    try:
        config, checker, directory = configuration(arguments.config)
        lock = Path(git(checker, "rev-parse", "--path-format=absolute", "--git-common-dir"))
        with locked(lock / "pr-duty" / config["repository"].lower().replace("/", "+")):
            control = directory / "control.json"
            if arguments.command in {"pause", "resume"}:
                save(control, {"paused": arguments.command == "pause", "updated_at": now()})
            paused = control.exists() and read(control)["paused"]
            if arguments.command not in {"scan", "tick"} or paused:
                print(json.dumps({"result": "paused" if paused else "active"}))
                return 0
            items = []
            for assignment in config.get("assignments", []):
                observed = observation(config, assignment, checker)
                if arguments.command == "tick":
                    observed = advance(config, checker, directory, assignment, observed)
                items.append(observed)
            report = {"result": "scanned" if items else "idle", "items": items, "at": now()}
            if arguments.command == "tick":
                save(directory / "last-tick.json", report)
                from scripts.pr_merge_duty_delivery import dispatch
                for assignment, observed in zip(config.get("assignments", []), items, strict=True):
                    observed["delivery"] = dispatch(config, directory, assignment, observed)
                save(directory / "last-tick.json", report)
            print(json.dumps(report, ensure_ascii=False))
            return 0
    except BlockingIOError:
        print(json.dumps({"result": "busy"}))
        return 75
    except forge.ForgeError as error:
        print(json.dumps({"result": "infrastructure-blocked", "detail": str(error),
                          "permission": error.permission, "retry_after": error.retry_after}))
        return 2
    except (KeyError, ValueError, OSError, TimeoutError, subprocess.SubprocessError) as error:
        print(json.dumps({"result": "needs-user", "detail": str(error)}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
