import json
from pathlib import Path
import re
import time
import uuid

from scripts import pr_task_github as forge
from scripts import pr_task_native as native
from scripts.pr_queue import checker_identity
from scripts.pr_task_state import ROOT, digest, git, load, locked, now, persist, record, save


def initialize(arguments):
    manual = arguments.mode == "manual"
    if manual and (arguments.socket or arguments.archive):
        raise ValueError("Manual tasks do not bind sockets or archive sessions")
    if not manual and (not arguments.socket or not arguments.thread):
        raise ValueError("Automatic tasks require --socket and the original --thread or CODEX_THREAD_ID")
    directory = Path(arguments.state).resolve()
    project = Path(arguments.project).resolve(strict=True)
    if project == ROOT.resolve():
        raise ValueError("Run the frozen checker CLI with --project pointing to a separate author worktree")
    checker = checker_identity()
    if directory.is_relative_to(project) or directory.is_relative_to(ROOT):
        raise ValueError("Task runtime records must stay outside source")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", arguments.repository):
        raise ValueError("Expected OWNER/REPOSITORY")
    if not 60 <= arguments.interval <= 86400 or not 1 <= arguments.max_rounds <= 20:
        raise ValueError("Use a polling interval of 60..86400 seconds and 1..20 repair rounds")
    if not 1 <= arguments.max_hours <= 720:
        raise ValueError("Task lifetime must be 1..720 hours")
    branch = git(project, "branch", "--show-current")
    author = arguments.thread or f"manual:{branch}"
    if not branch.startswith("codex/") or branch == arguments.base:
        raise ValueError("Register an isolated codex/ development branch")
    remote_url = git(project, "remote", "get-url", arguments.remote)
    match = re.fullmatch(r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)(.+?)(?:\.git)?",
                         remote_url)
    if not match or match[1].lower() != arguments.repository.lower():
        raise ValueError("The selected remote must identify the registered GitHub.com repository")
    repository = forge.api(f"repos/{arguments.repository}")["full_name"]
    login = forge.api("user")["login"]
    registry = Path(git(project, "rev-parse", "--path-format=absolute", "--git-common-dir")) / "pr-tasks"
    with locked(registry):
        owner_path = registry / f"{digest([repository, branch])}.json"
        if owner_path.exists():
            import json
            previous = Path(json.loads(owner_path.read_text())["directory"])
            if previous != directory and load(previous)["phase"] != "complete":
                raise ValueError(f"This branch already has a PR task: {previous}")
        with locked(directory):
            if (directory / "task.json").exists():
                state = load(directory)
                if (state["repository"], state["project"], state["route"]["thread_id"]) != (
                        repository, str(project), author):
                    raise ValueError("Existing task belongs to a different repository, worktree or session")
                if state.get("mode", "automatic") != arguments.mode:
                    raise ValueError("Existing task mode cannot change")
                if (state["checker"], state["branch"], state["base"], state["remote"]) != (
                        checker, branch, arguments.base, arguments.remote):
                    raise ValueError("Existing task checker, branch or base changed")
                return state
            route, anchor = ({"thread_id": author, "repair_cwd": str(project)}, None) if manual else (
                native.register(arguments.socket, arguments.thread, project))
            state = {
                "schema_version": 1, "mode": arguments.mode, "id": str(uuid.uuid4()), "directory": str(directory),
                "project": str(project), "repository": repository, "branch": branch,
                "checker": checker, "checker_project": str(ROOT),
                "base": arguments.base, "base_commit": git(project, "rev-parse", f"{arguments.remote}/{arguments.base}"),
                "remote": arguments.remote, "remote_url": remote_url,
                "route": route, "session_anchor": anchor, "archive": arguments.archive,
                "reviewers": sorted(set(arguments.reviewer or [login])),
                "checks": sorted(set(arguments.check or ["changes"])), "pr": None,
                "phase": "developing", "events": [], "completed": [], "active_event": None,
                "interval": arguments.interval, "max_rounds": arguments.max_rounds, "rounds": 0,
                "created_at": now(), "deadline": time.time() + arguments.max_hours * 3600,
                "next_poll_at": 0, "scheduler": None,
            }
            persist(state)
            save(owner_path, {"directory": str(directory), "thread": author})
            record(state, "registered", branch=branch)
            return state


def verify_checker(state):
    if checker_identity() != state.get("checker"):
        raise ValueError("Task checker changed; use the original frozen checker version and platform")


def rebind(state, socket_path, accept_input=False):
    if state["phase"] == "complete":
        raise ValueError("Completed tasks require a new registration")
    event = next((entry for entry in state["events"] if entry["id"] == state["active_event"]), None)
    route, anchor = native.register(socket_path, state["route"]["thread_id"], Path(state["project"]))
    with native.transport.connect_endpoint(route) as rpc:
        _, turn, users = native.latest(rpc, route)
        if not accept_input:
            native.owned(state, turn, users)
        if event and native.receipt_path(state, event).exists():
            path = native.receipt_path(state, event)
            receipt = json.loads(path.read_text())
            if receipt["state"] not in {"pending", "waiting_idle", "rejected"}:
                native.transport.observe(rpc, route, receipt)
                if not receipt.get("turn_id"):
                    raise ValueError("Unknown delivery is not proven on the new backend; do not resend")
            replacement = path.with_name(f"delivery-{event['id']}-{uuid.uuid4().hex}.json")
            receipt["route_sha256"] = digest(route)
            save(replacement, receipt)
            event["delivery_file"] = replacement.name
            event["delivery"] = receipt
    state.update(route=route, session_anchor=anchor, phase="watching" if state["pr"] else "developing",
                 next_poll_at=0)
    persist(state)
    record(state, "rebound", accepted_input=accept_input)
