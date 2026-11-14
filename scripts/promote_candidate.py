import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.validate_candidate import ROOT, acceptance_outcome, git, identity
from scripts.verify_package_startup import CHECKS

JOBS = {
    "Candidate (linux-x86_64)", "Candidate (macos-arm64)",
    "Finalize candidate", "Candidate gate",
}
PLATFORMS = {"linux-x86_64": ("linux", "x86_64"), "macos-arm64": ("darwin", "arm64")}
TARGETS = {"linux-x86_64": "x86_64-unknown-linux-gnu", "macos-arm64": "aarch64-apple-darwin"}


def github(*arguments):
    command = json.loads(os.environ.get("CODEX_GITHUB_COMMAND", '["gh"]'))
    if not isinstance(command, list) or not command or not all(isinstance(part, str) for part in command):
        raise ValueError("GitHub command must be a nonempty argument array")
    return subprocess.check_output([*command, *arguments], cwd=ROOT, text=True)


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


def validate_artifact(directory, source, label):
    record = json.loads((directory / "VALIDATION.json").read_text())
    expected = {key: source[key] for key in ("commit", "tree", "fingerprint")}
    observed = record["source"]
    if (record["scope"] != "candidate" or record["passed"] is not True
            or any(observed[key] != value for key, value in expected.items())
            or (observed["platform"], observed["machine"]) != PLATFORMS[label]):
        raise ValueError("Validation artifact does not match the candidate and platform")
    report = directory / "automated.json"
    automatic = json.loads(report.read_text())
    if (hashlib.sha256(report.read_bytes()).hexdigest() != record["automated_sha256"]
            or automatic["automated_passed"] is not True
            or automatic["build_sha256"] != source["fingerprint"]
            or [step["name"] for step in record["steps"]] != ["step-1", "step-2", "step-3"]
            or any(step["returncode"] != 0 for step in record["steps"][:2])
            or not acceptance_outcome(automatic, record["steps"][2]["returncode"])):
        raise ValueError("Candidate report is missing, unsuccessful or modified")
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--base", required=True)
    parser.add_argument("--run", required=True, type=int)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        raise ValueError("Promotion receipt must use a new path outside the source")
    source = identity()
    if source["commit"] != arguments.candidate or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Promotion requires the exact clean committed candidate")
    subprocess.run(["git", "merge-base", "--is-ancestor", arguments.base, arguments.candidate],
                   cwd=ROOT, check=True)
    api = f"repos/{arguments.repository}/actions/runs/{arguments.run}"
    run = json.loads(github("api", api))
    jobs = json.loads(github("api", api + "/jobs?filter=latest&per_page=100"))
    validate_run(run, jobs, source, arguments.repository)
    validations = {}
    with tempfile.TemporaryDirectory() as temporary:
        for label in PLATFORMS:
            directory = Path(temporary) / label
            github("run", "download", str(arguments.run), "--repo", arguments.repository,
                   "--name", f"acceptance-{label}", "--dir", str(directory))
            validations[label] = validate_artifact(directory, source, label)
    if identity() != source or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Candidate changed while checking remote validation")
    remote = git("ls-remote", "origin", "refs/heads/main").split()
    if len(remote) != 2 or remote[0] != arguments.base:
        raise ValueError("Main advanced; recombine and revalidate the candidate")
    subprocess.run(["git", "push", "origin", f"{arguments.candidate}:refs/heads/main"],
                   cwd=ROOT, check=True)
    if git("ls-remote", "origin", "refs/heads/main").split()[0] != arguments.candidate:
        raise ValueError("Main did not confirm the promoted candidate")
    receipt = {
        "schema_version": 1, "source": source, "base": arguments.base,
        "repository": arguments.repository, "run": arguments.run,
        "validated_platforms": validations,
        "promoted_at": datetime.now(timezone.utc).isoformat(),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"promoted": arguments.candidate, "receipt": str(output)}))


if __name__ == "__main__":
    main()
