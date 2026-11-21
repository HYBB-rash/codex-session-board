from datetime import datetime, timezone
import json
import re

from scripts.release_plugin import GitHub
from scripts.main_release import channels
from scripts.validate_candidate import git


def ensure_label(forge):
    labels = {item["name"] for item in forge.pages("labels")}
    for name, color in (("release-blocker", "b60205"), ("release-published", "0e8a16"),
                        ("merge-gate-gap", "d93f0b")):
        if name not in labels:
            forge.api("labels", "POST", name=name, color=color)


def incidents(forge):
    issues = forge.pages("issues?state=all&labels=release-blocker")
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


def comment_once(forge, number, marker, body):
    endpoint = f"issues/{number}/comments"
    expected = f"{marker}\n{body}"
    matches = [item for item in forge.pages(endpoint) if marker in item["body"]]
    if len(matches) > 1:
        raise ValueError("Release recovery comment appears more than once")
    created = matches[0] if matches else forge.api(endpoint, "POST", body=expected)
    observed = forge.api(f"issues/comments/{created['id']}")
    if observed["body"] != expected:
        raise ValueError("GitHub did not confirm the exact release recovery comment")
    return observed["html_url"]


def failure(repository, source, run, url, exercise=False, phase="unknown"):
    forge = GitHub(repository)
    previous = channels(forge)
    ensure_label(forge)
    marker = f"<!-- main-release:{'exercise' if exercise else 'failure'} {source} -->"
    issues = incidents(forge)
    matches = [issue for issue in issues if marker in (issue["body"] or "")]
    if len(matches) > 1:
        raise ValueError("Duplicate release incident records need manual reconciliation")
    body = (f"{marker}\n代码已合并，尚未发布。\nSource: `{source}`\nFailure-Phase: {phase}\n"
            f"失败流水线：{url}\n\n"
            "上一可用候选渠道保持原样：\n```json\n" + json.dumps(previous, indent=2) + "\n```\n\n"
            "请作者先区分基础设施问题和源码问题，在独立分支提交修复，"
            "提交消息使用 `Release-Fix: #本工单编号`，不要使用会自动关单的 closing keywords。"
            "先保留准确源码上的失败证据；若确认为合并门禁遗漏，"
            "给本工单加 `merge-gate-gap` 标签，补上对应回归，再按本地流程验证和合入。"
            "基础设施故障应记录原因后重试同一不可变候选，不能靠重跑掩盖业务失败。"
            "本工单保持打开，直到后续插件双平台发布成功。")
    issue = matches[0] if matches else forge.api(
        "issues", "POST", title=f"{'[演练] ' if exercise else ''}发布失败：{source[:12]} 已合并但尚未发布",
        body=body, **{"labels[]": "release-blocker"})
    observed = forge.api(f"issues/{issue['number']}")
    if marker not in (observed["body"] or ""):
        raise ValueError("GitHub did not confirm the source-bound release incident")
    return {"status": "failed", "source": source, "run": run, "issue": issue["html_url"],
            "exercise": exercise, "previous": previous,
            "recorded_at": datetime.now(timezone.utc).isoformat()}


def recovered(repository, source, run):
    forge = GitHub(repository)
    issues = incidents(forge)
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
        comment_once(forge, issue["number"], f"<!-- main-release:recovered {source} {run} -->",
                     f"双平台发布渠道已读回同一源码 `{source}`，发布故障已恢复。")
        forge.api(f"issues/{issue['number']}", "PATCH", state="closed")
        forge.api(f"issues/{issue['number']}/labels", "POST", **{"labels[]": "release-published"})
        closed.append(issue["number"])
    return closed
