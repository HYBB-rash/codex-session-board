import json
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts.verify_acceptance import fingerprint
from scripts.validation_bootstrap import parse
from scripts import validation_run as run, validation_report as report


def git(*arguments, root=None):
    return subprocess.check_output(["git", *arguments], cwd=root or ROOT, text=True).strip()


def identity(root=None):
    root = root or ROOT
    bindings = json.loads((root / "acceptance/bindings.json").read_text())
    return {
        "commit": git("rev-parse", "HEAD", root=root), "tree": git("rev-parse", "HEAD^{tree}", root=root),
        "fingerprint": fingerprint(root, bindings["fingerprint_paths"]),
        "platform": sys.platform, "machine": platform.machine(),
    }


def acceptance_outcome(report, returncode):
    requirements = report["requirements"]
    tests = report["tests"]
    ready = report["release_ready"]
    return (
        report["scope"] == "automated" and report["automated_passed"] is True
        and type(ready) is bool and report["passed"] is True
        and returncode == 0 and bool(tests) and bool(requirements)
        and all(test["status"] == "pass" for test in tests)
        and all(item["status"] in ("pass", "pending") for item in requirements)
        and ready == all(item["status"] == "pass" for item in requirements)
    )


def commands_for(scope, output):
    if scope == "preflight":
        return [["python3", "scripts/package_plugin.py", "--preflight",
                 "--output", str(output / "source-package")]]
    return [
        ["./scripts/check"],
        ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
        ["python3", "scripts/verify_acceptance.py", "--scope", "automated",
         "--report", str(output / "automated.json")],
    ]


def candidate_outcome(output, record):
    path = output / "automated.json"
    automatic = json.loads(path.read_text())
    if automatic["build_sha256"] != record["source"]["fingerprint"]:
        raise run.CheckFailed("自动报告与候选身份不一致")
    record["pending"] = [item["id"] for item in automatic["requirements"] if item["status"] == "pending"]
    record["automated_sha256"] = report.digest(path)
    if not acceptance_outcome(automatic, record["steps"][-1]["returncode"]):
        raise run.CheckFailed("自动报告未通过")
    record["release_ready"] = automatic["release_ready"]


def main():
    arguments = parse()
    output = arguments.output.resolve() if arguments.output else Path("<report-directory>")
    commands = commands_for(arguments.scope, output)
    if arguments.plan:
        print(json.dumps({"scope": arguments.scope, "executed": False,
                          "commands": commands},
                         ensure_ascii=False, indent=2))
        return 0
    record = run.begin(arguments, ROOT, arguments.scope)
    before = None
    try:
        before = identity()
        child = run.prepare(ROOT, output, record, before, commands)
        try:
            run.steps(ROOT, output, record, commands, child, arguments.timeout)
        except run.CheckFailed as error:
            run.reject(output, record, error, 1)
        if arguments.scope == "candidate" and len(record["steps"]) == len(commands):
            candidate_outcome(output, record)
    except KeyboardInterrupt:
        run.reject(output, record, "验证被中断", 130)
    except run.CheckFailed as error:
        run.reject(output, record, error, 1)
    except Exception as error:
        run.reject(output, record, error, 3)
    unchanged = None
    if before is not None:
        try:
            unchanged = identity() == before and not git("status", "--porcelain", "--untracked-files=all")
        except Exception as error:
            run.reject(output, record, error, 3)
    return run.finish(output, record, unchanged)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"候选验证失败：{error}", file=sys.stderr)
        sys.exit(3)
