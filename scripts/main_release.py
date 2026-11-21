import argparse
import base64
from datetime import datetime, timezone
import json
from pathlib import Path
import os
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from scripts.main_release_evidence import ROOT, implementation, select, verify_release
from scripts.promote_candidate import git, github
from scripts.release_plugin import GitHub
from scripts.package_plugin import TARGETS
from scripts.integration_plan import ancestor


def dispatch(repository, source):
    result = {"status": "pending", "requested_source": source,
              "requested_at": datetime.now(timezone.utc).isoformat()}
    for attempt in range(1, 4):
        try:
            github("workflow", "run", "plugin-release.yml", "--repo", repository, "--ref", "main", timeout=10)
            result.update(trigger="workflow_dispatch", attempts=attempt)
            return result
        except Exception as error:
            result.update(dispatch_error=str(error), attempts=attempt)
    result["retry"] = "./scripts/release main dispatch --repository " + repository
    result["automatic_retry"] = "plugin-release.yml scheduled reconciliation"
    try:
        from scripts.main_release_feedback import failure
        GitHub(repository).api(
            f"statuses/{source}", "POST", state="failure", context="Plugin release",
            description="代码已合并，发布触发失败；自动调和将重试",
            target_url=f"https://github.com/{repository}/actions/workflows/plugin-release.yml")
        result["feedback"] = failure(repository, source, "dispatch",
                                     f"https://github.com/{repository}/actions/workflows/plugin-release.yml",
                                     phase="dispatch")
    except Exception as error:
        result["feedback_error"] = str(error)
    return result


def channels(forge):
    result = {}
    for target in sorted(TARGETS):
        saved = forge.api(f"contents/RELEASE.json?ref=candidate%2F{target}")
        if saved is None:
            result[target] = None
        else:
            result[target] = json.loads(base64.b64decode(saved["content"]))
    return result


def already_published(repository, selected):
    forge = GitHub(repository)
    published = channels(forge)
    if not all(item and item["source_commit"] == selected["source"]
               and item["version"] == selected["version"] and item["target"] == target
               for target, item in published.items()):
        return False
    saved = forge.release(selected["tag"])
    if not saved or saved["draft"] or not saved.get("immutable"):
        return False
    commit = forge.api(f"commits/{selected['tag']}")
    return commit["sha"] == selected["source"]


def status(repository, source):
    git("fetch", "--quiet", "origin", "main")
    merged = ancestor(source, git("rev-parse", "FETCH_HEAD"))
    forge = GitHub(repository)
    published = channels(forge)
    statuses = forge.api(f"commits/{source}/statuses?per_page=100")
    latest = next((entry for entry in statuses if entry["context"] == "Plugin release"), None)
    canonical = implementation(source)
    origins = {item["source_commit"] for item in published.values() if item}
    coherent = len(origins) == 1 and all(published.values())
    covered = coherent and ancestor(canonical, next(iter(origins)))
    complete = covered and canonical in origins
    state = ("published" if complete else "covered" if covered else
             {"success": "inconsistent", "failure": "failed", "pending": "pending"}.get(
                 latest["state"] if latest else "", "pending"))
    return {"source": source, "merge_status": "merged" if merged else "not_merged", "published": published,
            "observed_at": datetime.now(timezone.utc).isoformat(),
            "release_status": state if merged else "not_requested",
            "covered_by": next(iter(origins)) if covered else None,
            "latest_attempt": latest}


def record(repository, source, state, run, exercise=False, phase="unknown"):
    if state == "success":
        current = channels(GitHub(repository))
        if not all(item and item["source_commit"] == source for item in current.values()):
            raise ValueError("Both delivery channels must confirm the exact published source")
    if exercise:
        if state == "failure":
            from scripts.main_release_feedback import failure
            return failure(repository, source, run,
                           f"https://github.com/{repository}/actions/runs/{run}", exercise=True, phase=phase)
        return {"source": source, "status": state, "exercise": True, "run": run}
    forge = GitHub(repository)
    url = f"https://github.com/{repository}/actions/runs/{run}"
    descriptions = {"pending": "代码已合并，插件发布中", "success": "双平台不可变插件已发布",
                    "failure": "代码已合并，尚未发布；保留上一可用版本"}
    forge.api(f"statuses/{source}", "POST", state=state, context="Plugin release",
              description=descriptions[state], target_url=url)
    if state == "failure":
        from scripts.main_release_feedback import failure
        return failure(repository, source, run, url, phase=phase)
    result = {"source": source, "status": state, "run": run,
              "observed_at": datetime.now(timezone.utc).isoformat()}
    if state == "success":
        from scripts.main_release_feedback import recovered
        result["published"] = current
        result["resolved_incidents"] = recovered(repository, source, run)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("select", "verify", "dispatch", "status", "record"))
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    parser.add_argument("--source")
    parser.add_argument("--checkout", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--state", choices=("pending", "success", "failure"))
    parser.add_argument("--run", default=os.environ.get("GITHUB_RUN_ID"))
    parser.add_argument("--exercise", action="store_true")
    parser.add_argument("--phase", default="unknown")
    arguments = parser.parse_args()
    repository = arguments.repository or json.loads(
        github("repo", "view", "--json", "nameWithOwner"))["nameWithOwner"]
    source = arguments.source or git("rev-parse", "HEAD")
    if arguments.action == "select":
        result = select(repository, arguments.source)
        result["published"] = str(not (arguments.exercise or arguments.source)
                                  and already_published(repository, result)).lower()
    elif arguments.action == "verify":
        if arguments.output is None or not arguments.run:
            parser.error("verify needs an external evidence directory and a publication run ID")
        result = verify_release(repository, arguments.output, arguments.checkout, source, arguments.run)
    elif arguments.action == "dispatch":
        result = dispatch(repository, source)
    elif arguments.action == "status":
        result = status(repository, source)
    else:
        if not arguments.state or not arguments.run:
            parser.error("record needs --state and --run")
        result = record(repository, source, arguments.state, arguments.run, arguments.exercise,
                        arguments.phase)
    if arguments.github_output:
        with arguments.github_output.open("a") as stream:
            for name in ("main", "source", "version", "tag", "published"):
                if name in result:
                    stream.write(f"{name}={result[name]}\n")
    if arguments.output and arguments.action != "verify":
        arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
