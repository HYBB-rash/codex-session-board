from datetime import datetime, timezone
import json
import re
import time

from scripts import pr_task_github as forge
from scripts.pr_task_publish import comment_once
from scripts.release_plugin import GitHub
from scripts.main_release import channels
from scripts.promote_candidate import git


def ensure_label(repository):
    prefix = f"repos/{repository}"
    labels = forge.pages(f"{prefix}/labels", time.monotonic() + 60)
    for name, color in (("release-blocker", "b60205"), ("release-published", "0e8a16"),
                        ("merge-gate-gap", "d93f0b")):
        if not any(item["name"] == name for item in labels):
            forge.api(f"{prefix}/labels", "--method", "POST", "-f", f"name={name}",
                      "-f", f"color={color}")


def incidents(repository):
    issues = forge.pages(f"repos/{repository}/issues?state=all&labels=release-blocker",
                         time.monotonic() + 60)
    return [issue for issue in issues if not any(
        label["name"] == "release-published" for label in issue.get("labels", []))]


def repair_covers(issue, base, source):
    commits = git("log", "--format=%B", f"{base}..{source}")
    paths = git("diff", "--name-only", base, source).splitlines()
    regression = any(path.startswith(("tests/", "acceptance/test_", "scripts/check-"))
                     and path != "acceptance/bindings.json" for path in paths)
    reference = rf"Release-Fix:\s+#{issue['number']}\b"
    gap = any(label["name"] == "merge-gate-gap" for label in issue.get("labels", []))
    return bool(re.search(reference, commits, re.IGNORECASE)) and (not gap or regression)


def affected_prs(repository, source, previous):
    commits = [source]
    origins = {item["source_commit"] for item in previous.values() if item}
    if len(origins) == 1:
        baseline = origins.pop()
        comparison = forge.api(f"repos/{repository}/compare/{baseline}...{source}")
        if comparison["total_commits"] > len(comparison["commits"]):
            raise ValueError("Release comparison is incomplete; no partial author feedback accepted")
        commits = [item["sha"] for item in comparison["commits"]]
    result = {}
    for commit in commits:
        for item in forge.pages(f"repos/{repository}/commits/{commit}/pulls",
                                time.monotonic() + 60):
            if item["merged_at"] and item["base"]["ref"] == "main":
                result[item["number"]] = item
    return result


def failure(repository, source, run, url, exercise=False, phase="unknown"):
    previous = channels(GitHub(repository))
    ensure_label(repository)
    marker = f"<!-- main-release:{'exercise' if exercise else 'failure'} {source} -->"
    issues = incidents(repository)
    matches = [issue for issue in issues if marker in (issue["body"] or "")]
    if len(matches) > 1:
        raise ValueError("Duplicate release incident records need manual reconciliation")
    body = (f"{marker}\n代码已合并，尚未发布。\nSource: `{source}`\nFailure-Phase: {phase}\n"
            f"失败流水线：{url}\n\n"
            "上一可用候选渠道保持原样：\n```json\n" + json.dumps(previous, indent=2) + "\n```\n\n"
            "请作者先区分基础设施问题和源码问题，提交独立修复 PR，"
            "提交消息使用 `Release-Fix: #本工单编号`，不要使用会自动关单的 closing keywords。"
            "先保留准确源码上的失败证据；若确认为合并门禁遗漏，"
            "给本工单加 `merge-gate-gap` 标签，并补进合并前的对应回归，再修复和验证。"
            "基础设施故障应记录原因后重试同一不可变候选，不能靠重跑掩盖业务失败。"
            "本工单保持打开，直到后续插件双平台发布成功。")
    issue = matches[0] if matches else forge.api(
        f"repos/{repository}/issues", "--method", "POST", "-f",
        f"title={'[演练] ' if exercise else ''}发布失败：{source[:12]} 已合并但尚未发布", "-f", f"body={body}",
        "-f", "labels[]=release-blocker")
    observed = forge.api(f"repos/{repository}/issues/{issue['number']}")
    if marker not in (observed["body"] or ""):
        raise ValueError("GitHub did not confirm the source-bound release incident")
    receipts = []
    for number, pr in ({} if exercise else affected_prs(repository, source, previous)).items():
        comment = comment_once(repository, number, f"<!-- main-release:author {source} {run} -->",
                               f"@{pr['user']['login']} 代码已合并，尚未发布。"
                               f"\nSource: `{source}`\n失败：{url}\n修复工单：{issue['html_url']}"
                               "\n请在新任务中提交修复 PR，并为门禁遗漏补合并前回归；原 PR 无须重新打开。")
        receipts.append({"pr": number, "comment": comment, "delivery": "confirmed",
                         "accepted": False})
    return {"status": "failed", "source": source, "run": run, "issue": issue["html_url"],
            "exercise": exercise,
            "previous": previous, "authors": receipts,
            "recorded_at": datetime.now(timezone.utc).isoformat()}


def require_recovery(repository, base):
    issues = incidents(repository)
    unresolved = [issue for issue in issues
                  if "<!-- main-release:failure " in (issue["body"] or "")]
    if not unresolved:
        return
    for issue in unresolved:
        if not repair_covers(issue, base, "HEAD"):
            raise ValueError(f"Main has an unresolved release incident #{issue['number']}; "
                             "merge a Release-Fix first; confirmed gate gaps also need a regression")


def recovered(repository, source, run):
    issues = incidents(repository)
    closed = []
    for issue in issues:
        match = re.search(r"<!-- main-release:(?:failure|exercise) ([0-9a-f]{40}) -->", issue["body"] or "")
        if not match:
            continue
        try:
            git("merge-base", "--is-ancestor", match[1], source)
        except Exception:
            continue
        if (any(label["name"] == "merge-gate-gap" for label in issue.get("labels", []))
                and not repair_covers(issue, match[1], source)):
            continue
        comment_once(repository, issue["number"], f"<!-- main-release:recovered {source} {run} -->",
                     f"双平台发布渠道已读回同一源码 `{source}`；发布故障已恢复。"
                     "\n这不表示已安装、重载或完成客户端验收。")
        forge.api(f"repos/{repository}/issues/{issue['number']}", "--method", "PATCH",
                  "-f", "state=closed")
        forge.api(f"repos/{repository}/issues/{issue['number']}/labels", "--method", "POST",
                  "-f", "labels[]=release-published")
        closed.append(issue["number"])
    return closed
