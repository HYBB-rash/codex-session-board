from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import pr_plan
from scripts.integration_plan import bound_inventory
from scripts.promote_candidate import github
from scripts.validate_candidate import identity


def now():
    return datetime.now(timezone.utc).isoformat()


def save(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def git(project, *arguments):
    return subprocess.check_output(["git", *arguments], cwd=project, text=True).strip()


def clean(project, expected):
    if (git(project, "rev-parse", "HEAD") != expected
            or git(project, "status", "--porcelain", "--untracked-files=all")
            or not git(project, "branch", "--show-current").startswith("codex/")):
        raise ValueError("Queue needs its exact clean dedicated codex/ worktree")


def checker_identity():
    if git(ROOT, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Queue checker must remain clean")
    return identity(ROOT)


def verify_checker(state):
    if checker_identity() != state["checker"]:
        raise ValueError("Queue checker identity changed; use the original frozen checker")


def initialize(manifest, directory):
    checker = checker_identity()
    state = json.loads(manifest.read_text())
    project = Path(state["project"]).resolve(strict=True)
    if project == ROOT.resolve():
        raise ValueError("Queue source needs an independent checker worktree")
    if directory.exists() or directory.is_relative_to(project) or directory.is_relative_to(ROOT):
        raise ValueError("Use a new queue directory outside source and checker")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", state["repository"]):
        raise ValueError("Expected an explicit owner/repository")
    if state.get("feedback_mode", "native") not in {"native", "github"}:
        raise ValueError("Feedback mode must be native or github")
    base = git(project, "rev-parse", f"{state['base']}^{{commit}}")
    clean(project, base)
    known = set()
    for item in state["items"]:
        number = item["number"]
        if type(number) is not int or number <= 0 or number in known:
            raise ValueError("PR numbers must be distinct positive integers")
        if not set(item.get("dependencies", [])) <= known:
            raise ValueError("Dependencies must precede their dependent PR")
        known.add(number)
        item["head"] = git(project, "rev-parse", f"{item['head']}^{{commit}}")
        item["base"] = git(project, "rev-parse", f"{item.get('base', base)}^{{commit}}")
        if not item["route"]["thread_id"]:
            raise ValueError("PR must identify its original author conversation")
        item.update(status="submitted", attempts=[])
    if not known:
        raise ValueError("Queue needs at least one PR")
    state.update(schema_version=1, project=str(project), base=base, initial_base=base, prefix=base,
                 checker=checker, checker_project=str(ROOT), started_at=now(), events=[],
                 batch_merged=False, release_status="not_requested",
                 batch_delivered=False, all_items_merged=False)
    directory.mkdir(parents=True)
    save(directory / "queue.json", state)
    return state


def verify_entry(state, item):
    verify_checker(state)
    review = json.loads(Path(item["review"]).read_text())
    if (review.get("head") != item["head"] or review.get("passed") is not True
            or not review.get("reviewer")
            or review["reviewer"] == item["route"]["thread_id"]):
        raise ValueError("Current PR head needs an independent passing review")
    verify_pr_validation(item, state["checker"])


def verify_pr_validation(item, checker):
    directory = Path(item["validation"])
    record = json.loads((directory / "VALIDATION.json").read_text())
    project = Path(record["project"]).resolve(strict=True)
    source = identity(project)
    strategy = pr_plan.plan(project, item["base"], stage=record.get("stage", "ci"))
    if (record.get("scope") != "pr" or record.get("passed") is not True
            or record.get("release_ready") is not False or record.get("acceptance_status") != "not_performed"
            or source["commit"] != item["head"] or record.get("source") != source
            or record.get("checker") != checker
            or record.get("plan") != strategy
            or git(project, "status", "--porcelain", "--untracked-files=all")):
        raise ValueError("PR evidence must identify the current clean source and queue checker")
    names = ["project-check", "all-target-clippy", "source-package"]
    names.append("related-tests")
    if [step["name"] for step in record["steps"]] != names:
        raise ValueError("PR evidence omitted required delivery steps")
    for step in record["steps"]:
        if (step["returncode"] != 0 or hashlib.sha256(
                (directory / f"{step['name']}.log").read_bytes()).hexdigest() != step["log_sha256"]):
            raise ValueError("PR step failed or its transcript changed")
    tests = directory / "TESTS.json"
    if hashlib.sha256(tests.read_bytes()).hexdigest() != record["tests_sha256"]:
        raise ValueError("PR test report changed")
    bindings = json.loads((project / "acceptance/bindings.json").read_text())
    pr_plan.validate_report(json.loads(tests.read_text()), strategy, source,
                            bound_inventory(bindings, source["platform"]))


def comment(state, item, feedback):
    marker = f"<!-- pr-queue:{feedback['id']} -->"
    endpoint = f"repos/{state['repository']}/issues/{item['number']}/comments"
    pages = json.loads(github("api", "--paginate", "--slurp", endpoint + "?per_page=100"))
    existing = next((entry for page in pages for entry in page
                     if marker in entry.get("body", "")), None)
    if existing:
        return existing["html_url"]
    label = "历史演练" if state.get("rehearsal") else "合并队列"
    body = (f"{marker}\n{label}暂未通过，请原作者修复后重新交付。\n\n"
            f"PR head: `{feedback['head']}`\n组合前置提交: `{feedback['prefix']}`\n"
            f"问题: {feedback['detail']}\n"
            f"冲突文件: {', '.join(feedback['files']) or '无文本冲突'}\n"
            f"复现: `git merge --no-ff --no-commit {feedback['head']}`，"
            f"在前置提交的独立工作树执行。\n"
            f"请求编号: `{feedback['id']}`")
    return json.loads(github("api", endpoint, "--method", "POST", "--raw-field", f"body={body}"))["html_url"]


def notify(state, item, directory):
    verify_checker(state)
    feedback = item["feedback"]
    folder = directory / feedback["id"]
    folder.mkdir(exist_ok=True)
    if not feedback.get("comment"):
        try:
            feedback["comment"] = comment(state, item, feedback)
        except Exception as error:
            feedback.setdefault("errors", []).append({"at": now(), "comment": str(error)})
    if state.get("feedback_mode") == "github":
        feedback["notification"] = "github_comment_posted" if feedback.get("comment") else "pending"
        save(folder / "feedback.json", feedback)
        save(directory / "queue.json", state)
        return
    try:
        request = {"id": feedback["id"], "body": feedback["body"]}
        save(folder / "request.json", request)
        save(folder / "route.json", item["route"])
        receipt_path = folder / "receipt.json"
        previous = json.loads(receipt_path.read_text()) if receipt_path.exists() else {"state": "pending"}
        command = "send" if previous["state"] in ("pending", "waiting_idle", "rejected") else "poll"
        process = subprocess.run([
            sys.executable, str(ROOT / "scripts/pr_queue_transport.py"), command,
            "--route", str(folder / "route.json"), "--request", str(folder / "request.json"),
            "--output", str(receipt_path),
        ], text=True, capture_output=True, timeout=60)
        with (folder / "transport.log").open("a") as stream:
            stream.write(process.stdout + process.stderr)
        if not receipt_path.exists():
            raise ValueError("Transport produced no receipt")
        receipt = json.loads(receipt_path.read_text())
        if (receipt.get("thread_id") != item["route"]["thread_id"]
                or receipt.get("request_id") != feedback["id"]
                or receipt.get("body_sha256") != hashlib.sha256(feedback["body"].encode()).hexdigest()
                or receipt.get("state") not in (
                    "pending", "waiting_idle", "sent", "acknowledged", "author_turn_completed",
                    "author_failed", "rejected", "delivery_unknown")):
            raise ValueError("Transport did not prove the intended author turn")
        acknowledged = receipt["state"] in ("acknowledged", "author_turn_completed")
        if acknowledged and (not receipt.get("turn_id") or not receipt.get("acknowledged_at")):
            raise ValueError("Author acknowledgement has no native turn evidence")
        feedback["receipt"] = receipt
        feedback["notification"] = "acknowledged" if acknowledged else receipt["state"]
    except Exception as error:
        feedback.setdefault("errors", []).append({"at": now(), "notification": str(error)})
        feedback.setdefault("notification", "pending")
    save(folder / "feedback.json", feedback)
    save(directory / "queue.json", state)


def reject(state, item, directory, kind, detail, files=()):
    payload = {"number": item["number"], "head": item["head"], "prefix": state["prefix"],
               "kind": kind, "detail": detail, "files": list(files)}
    payload["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL,
        json.dumps([state["repository"], state["started_at"], payload], sort_keys=True)))
    repair = shlex.quote(item["route"].get("repair_cwd", ""))
    checker = shlex.quote(str(ROOT / "scripts/validate"))
    payload["body"] = (
        f"用户授权的PR合并队列{'历史演练' if state.get('rehearsal') else '修复'}，"
        f"仓库{state['repository']}，PR#{item['number']}。\n"
        f"固定PR头{item['head']}，组合前置提交{state['prefix']}。\n"
        f"问题类型{kind}：{detail}\n冲突文件：{', '.join(payload['files']) or '无文本冲突'}。\n"
        f"请仅在指定修复工作树 {item['route'].get('repair_cwd', '')} 修复本PR，"
        "不要修改协调者工作树、真实main、正式插件或其他作者分支。"
        "先确认接单，再复现、修复、提交，完成独立审阅及当前源码交付检查。"
        "返回新提交、审阅与验证报告路径；不要降低原测试或覆盖首次失败证据。"
        f"\n固定检查器：{ROOT}，commit {state['checker']['commit']}。使用它检查修复树，不使用历史树的旧入口：\n"
        f'{checker} --scope pr --project {repair} --base {item["base"]} '
        f'--expect-head "$(git -C {repair} rev-parse HEAD)" --output "$(mktemp -d)/pr"\n'
        '审阅JSON：{"head":"最终完整SHA","passed":true,"reviewer":"独立审阅会话ID"}，'
        "reviewer必须不同于原作者会话。验证报告目录和审阅JSON均写在源码树之外。\n"
        f"补充修复要求：{item.get('repair_instructions', '无')}\n反馈编号：{payload['id']}")
    payload.update(created_at=now(), notification="pending")
    previous = next((entry for entry in item["attempts"] if entry.get("id") == payload["id"]), None)
    if previous:
        payload = previous
    else:
        item["attempts"].append(payload)
    item.update(status="needs-fix", feedback=payload)
    save(directory / "queue.json", state)
    notify(state, item, directory)


def commit(project, message):
    command = [shutil.which("agent-git") or "git", "commit", "-m", message]
    subprocess.run(command, cwd=project, check=True, stdout=sys.stderr)


def attempt(state, directory):
    verify_checker(state)
    project = Path(state["project"])
    clean(project, state["prefix"])
    for item in state["items"]:
        if item["status"] not in ("submitted", "blocked-dependency"):
            continue
        dependencies = [other for other in state["items"] if other["number"] in item.get("dependencies", [])]
        if any(other["status"] not in ("accepted", "merged") for other in dependencies):
            item["status"] = "blocked-dependency"
            continue
        prefix = state["prefix"]
        preview = subprocess.run([
            "git", "merge-tree", "--write-tree", "--name-only", "--no-messages", "-z", prefix, item["head"],
        ], cwd=project, text=True, capture_output=True)
        if preview.returncode not in (0, 1):
            raise RuntimeError("Git merge preview failed: " + preview.stderr)
        files = list(filter(None, preview.stdout.split("\0")[1:])) if preview.returncode else []
        blockers = ["Mechanical conflicts: " + ", ".join(files)] if preview.returncode else []
        kind = "merge" if preview.returncode else "delivery"
        if subprocess.run(["git", "merge-base", "--is-ancestor", item["head"], prefix], cwd=project).returncode == 0:
            blockers.append("PR is already in the prefix; no new integration occurred")
            kind = "source"
        try:
            verify_entry(state, item)
        except Exception as error:
            blockers.append("Delivery: " + str(error))
        if blockers:
            reject(state, item, directory, kind, "\n".join(blockers), files)
            continue
        merged = subprocess.run(["git", "merge", "--no-ff", "--no-commit", item["head"]],
                                cwd=project, text=True, capture_output=True)
        if merged.returncode:
            files = git(project, "diff", "--name-only", "--diff-filter=U", "-z").split("\0")
            if subprocess.run(["git", "rev-parse", "-q", "--verify", "MERGE_HEAD"],
                              cwd=project, stdout=subprocess.DEVNULL).returncode == 0:
                subprocess.run(["git", "merge", "--abort"], cwd=project, check=True)
            clean(project, prefix)
            if not any(files):
                raise RuntimeError("Git merge failed without a source conflict: " + merged.stderr)
            reject(state, item, directory, "merge", merged.stdout + merged.stderr, filter(None, files))
            continue
        try:
            commit(project, f"merge: queue PR {item['number']}")
        except Exception:
            subprocess.run(["git", "merge", "--abort"], cwd=project, check=True)
            raise
        state["prefix"] = git(project, "rev-parse", "HEAD")
        item.update(status="accepted", ingress=state["prefix"])
        item.pop("recombine_required", None)
        state["events"].append({"at": now(), "number": item["number"], "head": item["head"],
                                "prefix": prefix, "ingress": state["prefix"]})
        save(directory / "queue.json", state)
    save(directory / "queue.json", state)


def requeue(state, incoming, directory):
    verify_checker(state)
    item = next(entry for entry in state["items"] if entry["number"] == incoming["number"])
    if incoming.get("base", item["base"]) != item["base"]:
        raise ValueError("Repair cannot change its fixed comparison base")
    source = git(state["project"], "rev-parse", f"{incoming['head']}^{{commit}}")
    candidate_failure = item.get("feedback", {}).get("kind") == "candidate"
    evidence_only = item.get("feedback", {}).get("kind") == "delivery"
    revision = (state.get("feedback_mode") == "github" and not item.get("feedback")
                and item["status"] in {"submitted", "accepted", "blocked-dependency"})
    if ((item["status"] != "needs-fix" and not candidate_failure and not revision)
            or (source == item["head"] and not evidence_only)):
        raise ValueError("Requeue requires a changed source for a rejected PR")
    subprocess.run(["git", "merge-base", "--is-ancestor", item["head"], source],
                   cwd=state["project"], check=True)
    if not revision:
        notify(state, item, directory)
        expected_notification = "github_comment_posted" if state.get("feedback_mode") == "github" else "acknowledged"
        if item["feedback"].get("notification") != expected_notification:
            raise ValueError("The original author has not acknowledged this repair request")
    replacement = {**item, **{key: incoming[key] for key in ("head", "review", "validation")}}
    replacement["head"] = source
    replacement["base"] = item["base"]
    verify_entry(state, replacement)
    if evidence_only and source == item["head"] and item["feedback"].get("comment"):
        from scripts.pr_task_publish import comment_once
        from scripts.pr_task_github import api
        feedback = item["feedback"]
        comment_id = feedback["comment"].rstrip("/").rsplit("-", 1)[-1]
        original = api(f"repos/{state['repository']}/issues/comments/{comment_id}")
        if (f"<!-- pr-queue:{feedback['id']} -->" not in original["body"]
                or f"PR head: `{source}`" not in original["body"]):
            raise ValueError("Resolved feedback must match the original PR comment and source")
        body_sha256 = hashlib.sha256(original["body"].encode()).hexdigest()
        marker = f"<!-- pr-queue:resolved {feedback['id']} {source} {body_sha256} -->"
        comment_once(state["repository"], item["number"], marker,
                     "同一源码的独立审阅和交付证据已重新核验，解除该条交付反馈；原始失败评论保留。")
    if source != item["head"] and (item["status"] == "accepted" or item.get("ingress")):
        replacement["recombine_required"] = True
    item.update(replacement, status="submitted")
    item.pop("feedback", None)
    state.update(batch_merged=False, release_status="not_requested",
                 batch_delivered=False, all_items_merged=False)
    save(directory / "queue.json", state)


def validate_freeze(project, prefix, candidate, target=None):
    if git(project, "rev-list", "--parents", "-n", "1", candidate).split() != [candidate, prefix]:
        raise ValueError("Freeze must be one release-only commit directly after the reviewed prefix")
    paths = set(git(project, "diff", "--name-only", prefix, candidate).splitlines())
    required = {"Cargo.toml", "Cargo.lock", "project-rules.json"}
    plugins = set(git(project, "ls-tree", "-r", "--name-only", prefix, "--",
                      "plugin.json", ".codex-plugin/plugin.json").splitlines())
    required |= plugins
    target_path = "context/integration-target.json"
    optional = {target_path} if target is not None else set()
    if len(plugins) not in (0, 2) or not required <= paths or not paths <= required | optional:
        raise ValueError("Frozen candidate changed files outside release version manifests")
    if target is not None and json.loads(git(project, "show", f"{candidate}:{target_path}")) != target:
        raise ValueError("Rehearsal target differs from the fixed queue reference and integration base")
    for line in git(project, "diff", "--raw", prefix, candidate).splitlines():
        fields = line.split()
        added_target = target is not None and line.split("\t", 1)[1] == target_path and (
            fields[0] == ":000000" and fields[1] == "100644")
        if fields[0][1:] != fields[1] and not added_target:
            raise ValueError("Freeze cannot change file modes")
    before = tomllib.loads(git(project, "show", f"{prefix}:Cargo.toml"))
    after = tomllib.loads(git(project, "show", f"{candidate}:Cargo.toml"))
    old = before["package"]["version"]
    name = before["package"]["name"]
    rules = json.loads(git(project, "show", f"{prefix}:project-rules.json"))
    if rules["versioning"].get("release_only") is not True:
        raise ValueError("Freeze needs the existing release-only version policy")
    major, minor, patch = map(int, old.split("."))
    if rules["versioning"].get("rollover_after"):
        if minor >= 10 or patch >= 100:
            raise ValueError("Release version is outside the configured rollover range")
        major, minor, patch = (major + int(minor == 9), (minor + 1) % 10, 0) if patch == 99 else (
            major, minor, patch + 1)
    else:
        patch += 1
    version = f"{major}.{minor}.{patch}"
    before["package"]["version"] = version
    if before != after:
        raise ValueError("Cargo manifest changed beyond the next release version")
    lock = tomllib.loads(git(project, "show", f"{prefix}:Cargo.lock"))
    local = [package for package in lock["package"] if package["name"] == name and "source" not in package]
    if len(local) != 1 or local[0]["version"] != old:
        raise ValueError("Freeze needs exactly one matching local lockfile package")
    local[0]["version"] = version
    if lock != tomllib.loads(git(project, "show", f"{candidate}:Cargo.lock")):
        raise ValueError("Lockfile changed beyond the local release version")
    rules["versioning"]["feature_releases"].append({
        "after_commit": prefix, "from": old, "to": version, "decision": "冻结发布候选",
    })
    if rules != json.loads(git(project, "show", f"{candidate}:project-rules.json")):
        raise ValueError("Freeze changed policy beyond its exact release record")
    for path in paths & plugins:
        manifest = json.loads(git(project, "show", f"{prefix}:{path}"))
        if manifest["version"] != old:
            raise ValueError("Plugin version differs from the reviewed prefix")
        manifest["version"] = version
        if manifest != json.loads(git(project, "show", f"{candidate}:{path}")):
            raise ValueError("Plugin manifest changed beyond the release version")
    return version


def finish(state, directory):
    verify_checker(state)
    if any(item.get("recombine_required") for item in state["items"]):
        raise ValueError("Changed PR source requires mechanical recombine before freezing or promotion")
    if state.get("batch_merged"):
        state["merge_status"] = "merged"
        return
    reference = state.get("reference", "refs/heads/main")
    if ((state.get("rehearsal") and not re.fullmatch(
            r"refs/heads/codex/rehearsal-main-[A-Za-z0-9._-]+", reference))
            or (not state.get("rehearsal") and reference != "refs/heads/main")
            or not any(item["status"] == "accepted" for item in state["items"])):
        raise ValueError("Only an accepted queue with its permitted main reference can promote")
    project = Path(state["project"])
    candidate = git(project, "rev-parse", "HEAD")
    clean(project, candidate)
    frozen = state.get("frozen_candidate", {})
    if frozen.get("status") == "failed" and state["prefix"] == frozen["candidate"]:
        raise ValueError("Failed candidate needs an author repair merged before freezing a new version")
    prefix = frozen["reviewed_prefix"] if frozen.get("candidate") == candidate else state["prefix"]
    target = {"reference": reference, "base": state["base"]} if state.get("rehearsal") else None
    version = validate_freeze(project, prefix, candidate, target)
    state["frozen_candidate"] = {
        **(frozen if frozen.get("candidate") == candidate else {}),
        "candidate": candidate, "reviewed_prefix": prefix, "version": version, "status": "pending",
    }
    state["prefix"] = candidate
    save(directory / "queue.json", state)
    output = directory / f"candidate-{candidate}"
    command = [sys.executable, str(project / "scripts/integrate_candidate.py"),
               "--output", str(output), "--timeout", "0"]
    command += ["--resume"] if output.exists() else [
        "--base", state["base"], "--repository", state["repository"], "--started-at", state["started_at"],
    ]
    if reference != "refs/heads/main":
        command += ["--reference", reference]
    if not state.get("rehearsal"):
        command += ["--pr-gates", str(directory / "queue.json")]
    report = output / "integration.json"
    log = directory / f"candidate-{candidate}.log"
    state["integration"] = {"report": str(report), "log": str(log)}
    try:
        with log.open("a") as transcript:
            transcript.write(json.dumps({"started_at": now(), "command": command}) + "\n")
            transcript.flush()
            subprocess.run(command, cwd=project, check=True, stdout=transcript, stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError:
        details = json.loads(report.read_text()) if report.exists() else {}
        state["integration"].update({key: details.get(key) for key in
                                     ("run", "run_attempt", "workflow", "waiting_on")})
        error = details.get("last_error", {}).get("detail") or log.read_text()[-4000:]
        state["integration"]["error"] = error
        pending = error in {
            "Candidate is still pending; resume this same output directory",
            "Final PR checks are still pending; resume this same integration",
        }
        state["merge_status"] = "waiting-ci" if pending else details.get(
            "merge_status", "infrastructure-blocked")
        event = "candidate_pending" if pending else "candidate_blocked"
        state["events"].append({"at": now(), event: candidate, "report": str(report)})
        if re.fullmatch(r"Candidate run \d+ finished: failure", error):
            author = details.get("merge_status", "needs-author") == "needs-author"
            state["merge_status"] = "needs-author" if author else "infrastructure-blocked"
            if author:
                state["events"][-1] = {"at": now(), "candidate_failed": candidate, "report": str(report)}
            snapshot = output / f"failed-run-{details.get('run')}-{details.get('run_attempt')}.json"
            if not snapshot.exists():
                save(snapshot, details)
            state["frozen_candidate"]["status"] = "failed" if author else "infrastructure-blocked"
            state["frozen_candidate"]["failed_run"] = {
                "id": details.get("run"), "attempt": details.get("run_attempt"), "report": str(snapshot),
            }
            state["events"][-1]["failure_report"] = str(snapshot)
            for item in state["items"] if author else []:
                if item["status"] == "accepted":
                    reject(state, item, directory, "candidate",
                           f"组合候选 {candidate} 未通过，不能据此归责单一PR。"
                           f"共同复现材料 {report}；{error}")
                    item["status"] = "accepted"
        save(directory / "queue.json", state)
        if pending:
            return
        raise
    details = json.loads(report.read_text())
    state["integration"].update({key: details.get(key) for key in
                                 ("run", "run_attempt", "workflow", "waiting_on", "merge_status")})
    state["merge_status"] = details.get("merge_status")
    if state["merge_status"] == "waiting-ci":
        save(directory / "queue.json", state)
        return
    if state["merge_status"] != "merged" or details.get("source", {}).get("commit") != candidate:
        raise ValueError("Integration did not confirm this exact candidate entered main")
    for item in state["items"]:
        if item["status"] == "accepted":
            item["status"] = "merged"
            item["merged_at"] = details.get("completed_at") or now()
    state.setdefault("initial_base", state["base"])
    state["frozen_candidate"]["status"] = "merged"
    state.update(base=candidate, prefix=candidate, merged_at=now(), batch_merged=True,
                 release_status="pending", batch_delivered=False,
                 all_items_merged=all(item["status"] == "merged" for item in state["items"]))
    save(directory / "queue.json", state)


def refresh_release(state):
    if not state.get("batch_merged") or state.get("rehearsal"):
        return
    from scripts.main_release import status
    source = state["frozen_candidate"]["candidate"]
    observed = status(state["repository"], source)
    state.update(release_status=observed["release_status"], release=observed,
                 release_checked_at=now())


def retry_candidate(state, directory):
    verify_checker(state)
    frozen = state["frozen_candidate"]
    clean(Path(state["project"]), frozen["candidate"])
    previous = frozen.get("failed_run", {})
    if (frozen["status"] not in {"failed", "infrastructure-blocked"} or state["prefix"] != frozen["candidate"]
            or type(previous.get("id")) is not int or type(previous.get("attempt")) is not int):
        raise ValueError("Retry needs the exact unchanged failed candidate and its recorded workflow attempt")
    run = json.loads(github("api", f"repos/{state['repository']}/actions/runs/{previous['id']}"))
    if (run["id"] != previous["id"] or run["head_sha"] != frozen["candidate"]
            or run["run_attempt"] <= previous["attempt"]
            or run["status"] != "completed" or run["conclusion"] != "success"):
        raise ValueError("Rerun the recorded workflow, then retry-candidate after a newer attempt succeeds")
    frozen["status"] = "retry-ready"
    state["events"].append({"at": now(), "candidate_retry": frozen["candidate"],
                            "run": run["id"], "run_attempt": run["run_attempt"]})
    save(directory / "queue.json", state)
    finish(state, directory)


def main():
    from scripts.pr_queue_cli import main as cli
    return cli(sys.modules[__name__])


if __name__ == "__main__":
    sys.exit(main())
