import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts.validation_bootstrap import parse
from scripts import validation_run as run
from scripts.verify_acceptance import fingerprint


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def identity():
    return {"kind": "index", "commit": git("rev-parse", "HEAD"), "tree": git("write-tree")}


def commands_for():
    return [
        ["cargo", "run", "--quiet", "--locked", "--offline", "--example", "project_check", "--",
         "--pre-commit", "--baseline-repository", str(ROOT)],
        ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
        ["cargo", "test", "--locked", "--offline", "--", "--test-threads=1"],
    ]


def main():
    arguments = parse()
    commands = commands_for()
    if arguments.plan:
        print(json.dumps({"scope": "staged", "executed": False, "commands": commands}, indent=2))
        return 0
    output = arguments.output.resolve()
    record = run.begin(arguments, ROOT, "staged")
    snapshot = output / "source"
    before = None
    unchanged = None
    try:
        before = identity()
        index = git("rev-parse", "--path-format=absolute", "--git-path", "index")
        paths = git("ls-files", "-z").split("\0")
        snapshot.mkdir(mode=0o700)
        subprocess.run(["git", "checkout-index", "--all", f"--prefix={snapshot}/"], cwd=ROOT, check=True)
        source_sha = fingerprint(snapshot, [path for path in paths if path])
        child = run.prepare(snapshot, output, record, {**before, "fingerprint": source_sha},
                            commands, staged=True)
        for variable in git("rev-parse", "--local-env-vars").splitlines():
            child.pop(variable, None)
        run.steps(snapshot, output, record, commands[:1], dict(child, GIT_INDEX_FILE=index), arguments.timeout)
        run.steps(snapshot, output, record, commands[1:], child, arguments.timeout)
        unchanged = fingerprint(snapshot, [path for path in paths if path]) == source_sha
    except KeyboardInterrupt:
        run.reject(output, record, "用户中断验证", 130)
    except run.CheckFailed as error:
        run.reject(output, record, error, 1)
    except Exception as error:
        run.reject(output, record, error, 3)
    try:
        if before is not None:
            unchanged = identity() == before and unchanged is not False
        run.processes.cleanup_receipts(output, record["run_id"])
        if snapshot.exists():
            shutil.rmtree(snapshot)
    except Exception as error:
        run.reject(output, record, error, 3)
    return run.finish(output, record, unchanged)


if __name__ == "__main__":
    sys.exit(main())
