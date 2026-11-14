import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from scripts.validate_candidate import ROOT, acceptance_outcome, commands_for, git, identity
from scripts.verify_package_startup import CHECKS
from scripts.validation_report import verify_complete

JOBS = {
    "Candidate (linux-x86_64)", "Candidate (macos-arm64)",
    "Finalize candidate", "Candidate gate", "Classify integration",
}
PLATFORMS = {"linux-x86_64": ("linux", "x86_64"), "macos-arm64": ("darwin", "arm64")}
TARGETS = {"linux-x86_64": "x86_64-unknown-linux-gnu", "macos-arm64": "aarch64-apple-darwin"}


def github(*arguments):
    command = json.loads(os.environ.get("CODEX_GITHUB_COMMAND", '["gh"]'))
    if not isinstance(command, list) or not command or not all(isinstance(part, str) for part in command):
        raise ValueError("GitHub command must be a nonempty argument array")
    return subprocess.check_output([*command, *arguments], cwd=ROOT, text=True,
                                   stderr=subprocess.PIPE)


def validate_run(run, jobs, source, repository):
    if (run["head_sha"] != source["commit"] or run["status"] != "completed"
            or run["conclusion"] != "success" or run["repository"]["full_name"] != repository
            or run["path"] != ".github/workflows/plugin-candidate.yml"
            or run["event"] not in ("push", "workflow_dispatch")
            or not run["head_branch"].startswith("codex/integration-")):
        raise ValueError("Actions run does not identify a successful integration candidate")
    records = {job["name"]: job for job in jobs["jobs"]}
    if not JOBS.issubset(records) or any(
        records[name]["status"] != "completed" or records[name]["conclusion"] != "success"
        for name in JOBS
    ):
        raise ValueError("Required candidate jobs must each finish successfully")


def validate_artifact(directory, source, label, base=None):
    record = json.loads((directory / "VALIDATION.json").read_text())
    verify_complete(directory, record)
    expected = {key: source[key] for key in ("commit", "tree", "fingerprint")}
    observed = record["source"]
    if (record["scope"] != "candidate" or record["passed"] is not True
            or any(observed[key] != value for key, value in expected.items())
            or (observed["platform"], observed["machine"]) != PLATFORMS[label]):
        raise ValueError("Validation artifact does not match the candidate and platform")
    scoped = "integration_sha256" in record
    report = directory / ("integration.json" if scoped else "automated.json")
    automatic = json.loads(report.read_text())
    report_key = "integration_sha256" if scoped else "automated_sha256"
    if (hashlib.sha256(report.read_bytes()).hexdigest() != record[report_key]
            or record["release_ready"] is not automatic["release_ready"]
            or record["acceptance_status"] != "not_performed"
            or (record["environment"]["platform"], record["environment"]["machine"]) != PLATFORMS[label]
            or [step["name"] for step in record["steps"]] != ["step-1", "step-2", "step-3"]
            or any(step["returncode"] != 0 for step in record["steps"])):
        raise ValueError("Candidate report is missing, unsuccessful or modified")
    strategy = None
    if scoped:
        from scripts.integration_plan import bound_inventory, plan, validate_report
        if base is None:
            raise ValueError("Scoped validation needs the fixed main baseline")
        strategy = plan(base)
        if strategy["profile"] == "full":
            raise ValueError("This candidate requires the complete automated report")
        bindings = json.loads((ROOT / "acceptance/bindings.json").read_text())
        expected_source = {**source, "platform": PLATFORMS[label][0],
                           "machine": PLATFORMS[label][1]}
        if record.get("plan") != strategy:
            raise ValueError("Validation plan differs from actual candidate changes")
        validate_report(automatic, strategy, expected_source,
                        bound_inventory(bindings, PLATFORMS[label][0]))
        if record.get("not_run") != automatic["not_run"]:
            raise ValueError("Scoped validation must preserve its omitted checks")
    elif (automatic["automated_passed"] is not True
          or automatic["build_sha256"] != source["fingerprint"]
          or not acceptance_outcome(automatic, record["steps"][2]["returncode"])):
        raise ValueError("Full automated acceptance did not pass")
    pending = (["完整自动验收", "目标环境验收"] if scoped else
               [item["id"] for item in automatic["requirements"] if item["status"] == "pending"])
    commands = [step["arguments"] for step in record["steps"]]
    expected_commands = commands_for(argparse.Namespace(scope="candidate"), Path("report"), strategy)
    if (record["pending"] != pending or len(commands) != 3
            or commands[:2] != expected_commands[:2]
            or commands[2][:-1] != expected_commands[2][:-1]
            or Path(commands[2][-1]).name != Path(expected_commands[2][-1]).name):
        raise ValueError("Candidate steps or pending evidence do not preserve the required checks")
    for step in record["steps"]:
        log = directory / f"{step['name']}.log"
        if hashlib.sha256(log.read_bytes()).hexdigest() != step["log_sha256"]:
            raise ValueError("Validation transcript does not match its recorded digest")
    startup = json.loads((directory / "STARTUP.json").read_text())
    binary = startup["binary_sha256"]
    if (startup["scope"] != "package_startup" or startup["passed"] is not True
            or startup["cleanup_confirmed"] is not True or startup["checks"] != CHECKS
            or startup["source_commit"] != source["commit"]
            or startup["build_sha256"] != source["fingerprint"]
            or startup["target"] != TARGETS[label]
            or not isinstance(binary, str) or len(binary) != 64
            or any(character not in "0123456789abcdef" for character in binary)):
        raise ValueError("Packaged startup does not match the candidate and platform")
    record["package_startup"] = startup
    return record


def verify_saved_package(repository, label, source, startup, directory):
    from release_plugin import GitHub, asset_name, read_package
    from package_plugin import verify_package
    service = GitHub(repository)
    tag = "v" + startup["version"]
    release = service.release(tag)
    if not release or release["draft"] or release.get("immutable") is not True:
        raise ValueError("The actual candidate release must be immutable")
    directory.mkdir(parents=True)
    package = read_package(service, tag, TARGETS[label], source["commit"], directory)
    manifest = verify_package(package)
    for key in ("source_commit", "build_sha256", "binary_sha256", "version", "target"):
        if manifest[key] != startup[key]:
            raise ValueError("Actual package differs from its startup validation: " + key)
    archive = directory / asset_name(TARGETS[label])
    return {"manifest": manifest, "archive": str(archive),
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}


def collect_platform(repository, run, label, source, base, directory):
    validation = directory / "validation" / label
    github("run", "download", str(run), "--repo", repository,
           "--name", f"acceptance-{label}", "--dir", str(validation))
    record = validate_artifact(validation, source, label, base)
    package = verify_saved_package(
        repository, label, source, record["package_startup"], directory / "packages" / label)
    return label, record, package


def promote(candidate, base, run_id, repository, output):
    output = output.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        raise ValueError("Promotion receipt must use a new path outside the source")
    source = identity()
    if source["commit"] != candidate or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Promotion requires the exact clean committed candidate")
    subprocess.run(["git", "merge-base", "--is-ancestor", base, candidate],
                   cwd=ROOT, check=True)
    api = f"repos/{repository}/actions/runs/{run_id}"
    run = json.loads(github("api", api))
    jobs = json.loads(github("api", api + "/jobs?filter=latest&per_page=100"))
    validate_run(run, jobs, source, repository)
    output.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="verified-candidate-", dir=output.parent))
    validations, packages = {}, {}
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(collect_platform, repository, run_id, label, source,
                                  base, directory) for label in PLATFORMS]
        for future in futures:
            label, record, package = future.result()
            validations[label], packages[label] = record, package
    if identity() != source or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Candidate changed while checking remote validation")
    remote = git("ls-remote", "origin", "refs/heads/main").split()
    if len(remote) != 2 or remote[0] not in (base, candidate):
        raise ValueError("Main advanced; recombine and revalidate the candidate")
    if remote[0] == base:
        subprocess.run(["git", "push", "origin", f"{candidate}:refs/heads/main"],
                       cwd=ROOT, check=True)
    if git("ls-remote", "origin", "refs/heads/main").split()[0] != candidate:
        raise ValueError("Main did not confirm the promoted candidate")
    receipt = {
        "schema_version": 1, "source": source, "base": base,
        "repository": repository, "run": run_id,
        "validated_platforms": validations, "packages": packages,
        "artifacts": str(directory),
        "promoted_at": datetime.now(timezone.utc).isoformat(),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"promoted": candidate, "receipt": str(output)}))
    return receipt


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--base", required=True)
    parser.add_argument("--run", required=True, type=int)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    promote(arguments.candidate, arguments.base, arguments.run, arguments.repository, arguments.output)


if __name__ == "__main__":
    main()
