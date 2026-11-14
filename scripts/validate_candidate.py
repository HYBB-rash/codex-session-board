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


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def identity():
    bindings = json.loads((ROOT / "acceptance/bindings.json").read_text())
    return {
        "commit": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}"),
        "fingerprint": fingerprint(ROOT, bindings["fingerprint_paths"]),
        "platform": sys.platform, "machine": platform.machine(),
    }


def related_commands(base):
    changed = git("diff", "--name-only", f"{base}...HEAD").splitlines()
    targets = {
        path.split("/")[1].split(".")[0] for path in changed
        if path.startswith("tests/") and not path.startswith("tests/support/")
    }
    arguments = ["cargo", "test", "--locked", "--offline"]
    if any(path.startswith("src/") or path in ("Cargo.toml", "Cargo.lock") for path in changed):
        arguments.append("--lib")
        targets.update(("board_contract", "board_io", "native_authority", "remote_sessions"))
    if any(path == "index.html" or path.endswith(".mjs") for path in changed):
        targets.add("ui_layout")
    if any(path.startswith("scripts/") and path.endswith(".rs") for path in changed):
        arguments.extend(["--example", "project_check"])
    if any(path in (".githooks/pre-commit", "scripts/check", "scripts/validate") for path in changed):
        targets.add("git_hooks")
    for target in sorted(targets):
        arguments.extend(["--test", target])
    commands = [arguments + ["--", "--test-threads=1"]] if len(arguments) > 5 else []
    if any(path.endswith(".py") for path in changed):
        commands.append(["python3", "-m", "unittest", "discover", "-s", "acceptance"])
    return commands


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


def integration_strategy(arguments):
    if arguments.scope == "candidate" and arguments.integration_base:
        from scripts.integration_plan import plan
        return plan(arguments.integration_base)
    return None


def commands_for(arguments, output, strategy=None):
    commands = [
        ["./scripts/check"],
        ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
    ]
    scoped = strategy is not None and strategy["profile"] != "full"
    if arguments.scope == "preflight":
        commands = [["python3", "scripts/package_plugin.py", "--preflight",
                     "--output", str(output / "source-package")]]
    elif arguments.scope == "candidate":
        commands.append([
            "python3", "scripts/verify_integration.py", "--base", strategy["base"],
            "--report", str(output / "integration.json"),
        ] if scoped else [
            "python3", "scripts/verify_acceptance.py", "--scope", "automated",
            "--report", str(output / "automated.json"),
        ])
    else:
        commands.extend(related_commands(arguments.base))
    return commands


def candidate_outcome(output, record, strategy):
    scoped = strategy is not None and strategy["profile"] != "full"
    name = "integration" if scoped else "automated"
    automatic = json.loads((output / f"{name}.json").read_text())
    if scoped:
        from scripts.integration_plan import bound_inventory, validate_report
        bindings = json.loads((ROOT / "acceptance/bindings.json").read_text())
        try:
            validate_report(automatic, strategy, record["source"], bound_inventory(bindings, sys.platform))
        except ValueError as error:
            raise run.CheckFailed(str(error)) from error
        record["pending"] = ["完整自动验收", "目标环境验收"]
        record["not_run"] = automatic["not_run"]
    elif (not acceptance_outcome(automatic, record["steps"][-1]["returncode"])
          or automatic["build_sha256"] != record["source"]["fingerprint"]):
        raise run.CheckFailed("自动报告失败或与候选身份不一致")
    else:
        record["pending"] = [item["id"] for item in automatic["requirements"] if item["status"] == "pending"]
    record[f"{name}_sha256"] = report.digest(output / f"{name}.json")
    record["release_ready"] = automatic["release_ready"]


def main():
    arguments = parse()
    output = arguments.output.resolve() if arguments.output else Path("<report-directory>")
    if arguments.plan:
        strategy = integration_strategy(arguments)
        print(json.dumps({"scope": arguments.scope, "executed": False,
                          "plan": strategy, "commands": commands_for(arguments, output, strategy)},
                         ensure_ascii=False, indent=2))
        return 0
    record = run.begin(arguments, ROOT, arguments.scope)
    before = None
    try:
        before = identity()
        strategy = integration_strategy(arguments)
        if strategy is not None:
            record["plan"] = strategy
        commands = commands_for(arguments, output, strategy)
        child = run.prepare(ROOT, output, record, before, commands)
        run.steps(ROOT, output, record, commands, child, arguments.timeout)
        if arguments.scope == "candidate":
            candidate_outcome(output, record, strategy)
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
