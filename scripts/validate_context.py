import argparse
import json
from pathlib import Path
import re
import subprocess


def git(*arguments):
    return subprocess.check_output(["git", *arguments], text=True).strip()


def require_target(reference, base, branch, source="HEAD", git_command=git):
    rehearsal = branch.startswith("codex/integration-rehearsal-")
    if reference == "refs/heads/main":
        if rehearsal:
            raise ValueError("Rehearsal candidates require an explicit rehearsal target")
        return
    if (not rehearsal or not re.fullmatch(
            r"refs/heads/codex/rehearsal-main-[A-Za-z0-9][A-Za-z0-9._-]*", reference)
            or not re.fullmatch(r"[0-9a-f]{40}", base)):
        raise ValueError("Only fixed rehearsal targets are supported")
    git_command("check-ref-format", reference)
    configuration = json.loads(git_command("show", f"{source}:context/integration-target.json"))
    if configuration != {"reference": reference, "base": base}:
        raise ValueError("Candidate target configuration does not match the explicit target and base")
    git_command("merge-base", "--is-ancestor", base, source)


def require_remote_target(reference, allowed, git_command=git):
    remote = git_command("ls-remote", "origin", reference).split()
    if len(remote) != 2 or remote[1] != reference or remote[0] not in allowed:
        raise ValueError("Integration target advanced or is missing; recombine and revalidate")
    return remote[0]


def classification_target(branch, base):
    if not branch.startswith("codex/integration-rehearsal-"):
        return git("rev-parse", f"{base}^{{commit}}"), "refs/heads/main"
    configuration = json.loads(git("show", "HEAD:context/integration-target.json"))
    reference, fixed_base = configuration["reference"], configuration["base"]
    require_target(reference, fixed_base, branch)
    require_remote_target(reference, (fixed_base,))
    return fixed_base, reference


def context_only(base):
    subprocess.run(["git", "merge-base", "--is-ancestor", base, "HEAD"], check=True)
    paths = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", base, "HEAD"], text=True,
    ).split("\0")[:-1]
    if not paths or any(
        not path.startswith("context/") or Path(path).suffix not in (".md", ".yaml", ".json")
        for path in paths
    ):
        return False
    for reference in (base, "HEAD"):
        entries = subprocess.check_output(
            ["git", "ls-tree", "-rz", reference, "--", *paths], text=True,
        ).split("\0")[:-1]
        if any(entry.split()[0] != "100644" for entry in entries):
            return False
    return True


def require_base_gate(repository, base):
    prefix = f"repos/{repository}"
    remote = json.loads(subprocess.check_output(
        ["gh", "api", f"{prefix}/git/ref/heads/main"], text=True))
    if remote["object"]["sha"] != base:
        raise ValueError("Main advanced; rebase the records and validate again")
    pages = json.loads(subprocess.check_output([
        "gh", "api", "--paginate", "--slurp",
        f"{prefix}/commits/{base}/check-runs?check_name=Candidate%20gate&per_page=100",
    ], text=True))
    checks = [check for page in pages for check in page["check_runs"]]
    if not any(
        check["name"] == "Candidate gate" and check["head_sha"] == base
        and check["app"]["id"] == 15368 and check["status"] == "completed"
        and check["conclusion"] == "success"
        for check in checks
    ):
        raise ValueError("The unchanged main implementation has no successful Actions gate")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default="origin/main")
    parser.add_argument("--branch", default="")
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    base, reference = classification_target(arguments.branch, arguments.base)
    source = git("rev-parse", "HEAD")
    records_only = context_only(base) if reference == "refs/heads/main" else False
    if arguments.repository:
        if not records_only:
            raise ValueError("Context validation cannot admit implementation or workflow changes")
        require_base_gate(arguments.repository, base)
        subprocess.run(["./scripts/check"], check=True)
        subprocess.run(["sh", "scripts/verify_context.sh"], check=True)
    if git("status", "--porcelain", "--untracked-files=all") or git("rev-parse", "HEAD") != source:
        raise ValueError("Context validation requires an unchanged clean checkout")
    report = {
        "scope": "context" if arguments.repository else "classification",
        "context_only": records_only, "base": base, "source_commit": source, "reference": reference,
        "implementation_unchanged": records_only,
    }
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    if arguments.github_output:
        with arguments.github_output.open("a") as stream:
            stream.write(f"context_only={str(records_only).lower()}\nbase={base}\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
