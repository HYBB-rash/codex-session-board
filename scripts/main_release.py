import argparse
import base64
from datetime import datetime, timezone
import json
from pathlib import Path
import os
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from scripts.main_release_evidence import implementation, select, verify_merge
from scripts.promote_candidate import git, github
from scripts.release_plugin import GitHub
from scripts.package_plugin import TARGETS


def dispatch(repository, source):
    result = {"status": "pending", "requested_source": source,
              "requested_at": datetime.now(timezone.utc).isoformat()}
    try:
        github("workflow", "run", "plugin-release.yml", "--repo", repository, "--ref", "main", timeout=30)
        result["trigger"] = "workflow_dispatch"
    except Exception as error:
        result["dispatch_error"] = str(error)
        result["retry"] = "./scripts/release main dispatch --repository " + repository
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


def status(repository, source):
    forge = GitHub(repository)
    published = channels(forge)
    statuses = forge.api(f"commits/{source}/statuses?per_page=100")
    latest = next((entry for entry in statuses if entry["context"] == "Plugin release"), None)
    complete = all(record and record["source_commit"] == implementation(source)
                   for record in published.values())
    return {"source": source, "merge_status": "merged", "published": published,
            "release_status": "published" if complete else
                {"failure": "failed", "pending": "pending"}.get(
                    latest["state"] if latest else "", "pending"),
            "latest_attempt": latest}


def record(repository, source, state, run, exercise=False):
    if state == "success":
        current = channels(GitHub(repository))
        if not all(item and item["source_commit"] == source for item in current.values()):
            raise ValueError("Both delivery channels must confirm the exact published source")
    if exercise:
        if state == "failure":
            from scripts.main_release_feedback import failure
            return failure(repository, source, run,
                           f"https://github.com/{repository}/actions/runs/{run}", exercise=True)
        return {"source": source, "status": state, "exercise": True, "run": run}
    forge = GitHub(repository)
    url = f"https://github.com/{repository}/actions/runs/{run}"
    descriptions = {"pending": "代码已合并，插件发布中", "success": "双平台不可变插件已发布",
                    "failure": "代码已合并，尚未发布；保留上一可用版本"}
    forge.api(f"statuses/{source}", "POST", state=state, context="Plugin release",
              description=descriptions[state], target_url=url)
    if state == "failure":
        from scripts.main_release_feedback import failure
        return failure(repository, source, run, url)
    result = {"source": source, "status": state, "run": run}
    if state == "success":
        from scripts.main_release_feedback import recovered
        result["resolved_incidents"] = recovered(repository, source, run)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("select", "verify", "dispatch", "status", "record"))
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    parser.add_argument("--source")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--state", choices=("pending", "success", "failure"))
    parser.add_argument("--run", default=os.environ.get("GITHUB_RUN_ID"))
    parser.add_argument("--exercise", action="store_true")
    arguments = parser.parse_args()
    repository = arguments.repository or json.loads(
        github("repo", "view", "--json", "nameWithOwner"))["nameWithOwner"]
    source = arguments.source or git("rev-parse", "HEAD")
    if arguments.action == "select":
        result = select(repository)
    elif arguments.action == "verify":
        if arguments.output is None:
            parser.error("verify needs an external evidence directory")
        result = verify_merge(repository, arguments.output)
    elif arguments.action == "dispatch":
        result = dispatch(repository, source)
    elif arguments.action == "status":
        result = status(repository, source)
    else:
        if not arguments.state or not arguments.run:
            parser.error("record needs --state and --run")
        result = record(repository, source, arguments.state, arguments.run, arguments.exercise)
    if arguments.github_output:
        with arguments.github_output.open("a") as stream:
            for name in ("main", "source", "version", "tag"):
                if name in result:
                    stream.write(f"{name}={result[name]}\n")
    if arguments.output and arguments.action != "verify":
        arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
