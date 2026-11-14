import argparse
import json
from pathlib import Path
import subprocess


def git(*arguments):
    return subprocess.check_output(["git", *arguments], text=True).strip()


def context_only(base):
    subprocess.run(["git", "merge-base", "--is-ancestor", base, "HEAD"], check=True)
    paths = git("diff", "--no-renames", "--name-only", "-z", base, "HEAD").split("\0")[:-1]
    if not paths or any(
        not path.startswith("context/") or Path(path).suffix not in (".md", ".yaml", ".json")
        for path in paths
    ):
        return False
    for reference in (base, "HEAD"):
        entries = git("ls-tree", "-r", reference, "--", *paths).splitlines()
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
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    base = git("rev-parse", f"{arguments.base}^{{commit}}")
    source = git("rev-parse", "HEAD")
    records_only = context_only(base)
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
        "context_only": records_only, "base": base, "source_commit": source,
        "implementation_unchanged": records_only,
    }
    arguments.output.write_text(json.dumps(report, indent=2) + "\n")
    if arguments.github_output:
        with arguments.github_output.open("a") as stream:
            stream.write(f"context_only={str(records_only).lower()}\nbase={base}\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
