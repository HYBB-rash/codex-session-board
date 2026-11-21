from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

from scripts.release_plugin import GitHub
from scripts.validate_candidate import ROOT, acceptance_outcome, git, identity
from scripts.validation_report import verify_complete
from scripts.integration_plan import changed, context_path, modes


PLATFORMS = {"linux-x86_64": ("linux", "x86_64"), "macos-arm64": ("darwin", "arm64")}


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


def select(repository, source=None):
    if git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Release selection requires a clean checkout")
    observed = GitHub(repository).api("git/ref/heads/main")["object"]["sha"]
    subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, check=True)
    main = git("rev-parse", f"{observed}^{{commit}}")
    if main != observed:
        raise ValueError("Pinned main source is unavailable after fetching")
    if source is None:
        source = git("log", "--first-parent", "--diff-merges=first-parent", "-n", "1", "--format=%H",
                     "-G^version[[:space:]]*=", main, "--", "Cargo.toml")
    if not re.fullmatch(r"[0-9a-f]{40}", source):
        raise ValueError("Release validation requires a full main commit SHA")
    subprocess.run(["git", "merge-base", "--is-ancestor", source, main], cwd=ROOT, check=True)
    version = tomllib.loads(git("show", f"{source}:Cargo.toml"))["package"]["version"]
    return {"main": main, "source": source, "version": version, "tag": f"v{version}",
            "selected_at": datetime.now(timezone.utc).isoformat()}


def require_version(repository, base):
    from scripts.release_plugin import GitHub
    previous = tomllib.loads(git("show", f"{base}:Cargo.toml"))["package"]["version"]
    version = tomllib.loads(git("show", "HEAD:Cargo.toml"))["package"]["version"]
    if tuple(map(int, version.split("."))) <= tuple(map(int, previous.split("."))):
        raise ValueError("Freeze a fresh release version before validating a changed implementation")
    forge = GitHub(repository)
    if forge.api(f"git/ref/tags/v{version}") is not None:
        if forge.api(f"commits/v{version}")["sha"] != git("rev-parse", "HEAD"):
            raise ValueError("This release version already belongs to different immutable source")


def validation_attempts(forge, run_id):
    run = forge.api(f"actions/runs/{run_id}")
    if (run["path"] != ".github/workflows/plugin-release.yml"
            or run["repository"]["full_name"] != forge.repository):
        raise ValueError("Evidence must come from this repository's publication workflow")
    jobs = forge.api(f"actions/runs/{run_id}/jobs?filter=latest&per_page=100")["jobs"]
    attempts = {}
    for label in PLATFORMS:
        matching = [job for job in jobs if job["name"] == f"Validate release ({label})"]
        if (len(matching) != 1 or matching[0]["status"] != "completed"
                or matching[0]["conclusion"] != "success" or matching[0]["head_sha"] != run["head_sha"]):
            raise ValueError(f"Release validation did not succeed on {label}")
        attempts[label] = matching[0]["run_attempt"]
    return attempts


def platform_evidence(forge, run, source, label, attempt, directory):
    output = directory / label
    output.mkdir()
    artifact = f"release-validation-{label}-{attempt}"
    forge.gh("run", "download", str(run), "--name", artifact, "--dir", str(output))
    validation = output / "VALIDATION.json"
    record = json.loads(validation.read_text())
    verify_complete(output, record)
    observed = record["source"]
    expected = {key: source[key] for key in ("commit", "tree", "fingerprint")}
    if (record["scope"] != "candidate" or any(observed[key] != value for key, value in expected.items())
            or (observed["platform"], observed["machine"]) != PLATFORMS[label]
            or (record["environment"]["platform"], record["environment"]["machine"]) != PLATFORMS[label]):
        raise ValueError(f"Validation does not match the pinned release source and {label}")
    automatic = output / "automated.json"
    if hashlib.sha256(automatic.read_bytes()).hexdigest() != record["automated_sha256"]:
        raise ValueError(f"Automatic acceptance evidence changed for {label}")
    report = json.loads(automatic.read_text())
    commands = [step["arguments"] for step in record["steps"]]
    if (len(commands) != 3 or commands[:2] != [
            ["./scripts/check"], ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"]]
            or commands[2][:-1] != ["python3", "scripts/verify_acceptance.py", "--scope", "automated", "--report"]
            or Path(commands[2][-1]).name != "automated.json"
            or not acceptance_outcome(report, record["steps"][-1]["returncode"])
            or report["build_sha256"] != source["fingerprint"]
            or record["release_ready"] is not report["release_ready"]):
        raise ValueError(f"Full automated release validation did not pass on {label}")
    return label, {"artifact": artifact, "report": record,
                   "validation_sha256": hashlib.sha256(validation.read_bytes()).hexdigest()}


def verify_release(repository, directory, checkout, source_commit, run):
    source = identity(checkout)
    if source["commit"] != source_commit or git("status", "--porcelain", "--untracked-files=all", root=checkout):
        raise ValueError("Release verification requires a clean fixed main source")
    subprocess.run(["git", "merge-base", "--is-ancestor", source_commit, "origin/main"],
                   cwd=checkout, check=True)
    forge = GitHub(repository)
    attempts = validation_attempts(forge, run)
    directory.mkdir(parents=True, exist_ok=False)
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(platform_evidence, forge, run, source, label, attempt, directory)
                   for label, attempt in attempts.items()]
        reports = dict(future.result() for future in futures)
    if identity(checkout) != source:
        raise ValueError("Release source changed while checking publication evidence")
    result = {"source": source, "run": run, "platforms": reports,
              "verified_at": datetime.now(timezone.utc).isoformat()}
    (directory / "VALIDATED-RELEASE.json").write_text(json.dumps(result, indent=2) + "\n")
    return result
