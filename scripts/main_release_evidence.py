from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

from scripts.promote_candidate import ROOT, PLATFORMS, git, github, identity, validate_artifact, validate_run
from scripts.integration_plan import changed, context_path, modes


def implementation(main):
    source = main
    while True:
        parents = git("rev-list", "--parents", "-n", "1", source).split()[1:]
        if not parents:
            return source
        parent = parents[0]
        paths = changed(parent, source)
        if not paths or any(not context_path(path) for path in paths):
            return source
        before, after = modes(parent, paths), modes(source, paths)
        if any(before.get(path, after.get(path)) != "100644"
               or after.get(path, before.get(path)) != "100644" for path in paths):
            return source
        source = parent


def select(repository):
    if git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Release selection requires a clean checkout")
    subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, check=True)
    main = git("rev-parse", "FETCH_HEAD")
    observed = json.loads(github("api", f"repos/{repository}/git/ref/heads/main"))["object"]["sha"]
    if main != observed:
        raise ValueError("Main advanced during selection; retry before pinning a release")
    source = implementation(main)
    version = tomllib.loads(git("show", f"{source}:Cargo.toml"))["package"]["version"]
    return {"main": main, "source": source, "version": version, "tag": f"v{version}",
            "selected_at": datetime.now(timezone.utc).isoformat()}


def merge_run(repository, source):
    records = json.loads(github("api", f"repos/{repository}/actions/workflows/"
                               f"plugin-candidate.yml/runs?head_sha={source}&per_page=100"))
    matches = [run for run in records["workflow_runs"]
               if run["head_sha"] == source and run["head_branch"].startswith("codex/integration-")
               and run["event"] in ("push", "workflow_dispatch")]
    if not matches:
        raise ValueError("This main implementation has no frozen merge-candidate run")
    run = max(matches, key=lambda entry: entry["id"])
    jobs = json.loads(github("api", f"repos/{repository}/actions/runs/{run['id']}"
                            "/jobs?filter=latest&per_page=100"))
    validate_run(run, jobs, {"commit": source}, repository)
    return run


def platform_evidence(repository, run, source, label, directory):
    output = directory / label
    output.mkdir()
    github("run", "download", str(run), "--repo", repository,
           "--name", f"acceptance-{label}", "--dir", str(output))
    record = json.loads((output / "VALIDATION.json").read_text())
    validate_artifact(output, source, label, record.get("plan", {}).get("base"))
    return label, {"validation_sha256": hashlib.sha256(
        (output / "VALIDATION.json").read_bytes()).hexdigest(), "report": record}


def verify_merge(repository, directory):
    source = identity()
    if git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Release verification requires a clean fixed main source")
    subprocess.run(["git", "merge-base", "--is-ancestor", source["commit"], "origin/main"],
                   cwd=ROOT, check=True)
    run = merge_run(repository, source["commit"])
    directory.mkdir(parents=True, exist_ok=False)
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(platform_evidence, repository, run["id"], source, label, directory)
                   for label in PLATFORMS]
        reports = dict(future.result() for future in futures)
    if identity() != source:
        raise ValueError("Release source changed while checking merge evidence")
    result = {"source": source, "merge_run": run["id"], "platforms": reports,
              "verified_at": datetime.now(timezone.utc).isoformat()}
    (directory / "MERGE.json").write_text(json.dumps(result, indent=2) + "\n")
    return result
