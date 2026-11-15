from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys
import time

from scripts import pr_plan
from scripts.validate_candidate import ROOT, execute, git, identity
from scripts.integration_plan import bound_inventory


def save(path, record):
    path.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")


def validate(project, base, output, expected=None, additions=()):
    project, output = Path(project).resolve(strict=True), Path(output).resolve()
    if output.is_relative_to(project) or output.is_relative_to(ROOT):
        raise ValueError("PR delivery records must remain outside the source and checker")
    output.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    record = {"schema_version": 1, "scope": "pr", "passed": False, "release_ready": False,
              "acceptance_status": "not_performed", "project": str(project), "steps": [],
              "started_at": datetime.now(timezone.utc).isoformat()}
    report_path = output / "VALIDATION.json"
    save(report_path, record)

    def step(name, command):
        entry = {"name": name, "arguments": command, "returncode": None}
        record["steps"].append(entry)
        save(report_path, record)
        remaining = 1800 - (time.monotonic() - started)
        if remaining <= 0:
            raise TimeoutError("PR delivery exceeded its 1800 second total budget")
        entry.update(execute(name, command, output, root=project, timeout=remaining))
        save(report_path, record)
        if entry["returncode"] != 0:
            raise RuntimeError(f"PR check failed: {name}; see {output / (name + '.log')}")

    try:
        if (git("status", "--porcelain", "--untracked-files=all", root=project)
                or project != ROOT and git("status", "--porcelain", "--untracked-files=all", root=ROOT)):
            raise ValueError("PR delivery requires a clean committed checkout")
        source = identity(project)
        record.update(source=source, checker=identity())
        if expected and source["commit"] != git("rev-parse", f"{expected}^{{commit}}", root=project):
            raise ValueError("The requested PR head is not checked out")
        strategy = pr_plan.plan(project, base, additions)
        record["plan"] = strategy
        save(output / "PLAN.json", strategy)
        save(report_path, record)
        step("project-check", ["./scripts/check"])
        step("all-target-clippy", [
            "cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings",
        ])
        step("source-package", [
            sys.executable, str(project / "scripts/package_plugin.py"), "--preflight",
            "--output", str(output / "source-package"),
        ])
        if strategy["package_required"]:
            step("build-fixture", [
                "cargo", "build", "--locked", "--offline", "--example", "dev_fixture",
                "--message-format=json",
            ])
            fixtures = []
            for line in (output / "build-fixture.log").read_text().splitlines():
                try:
                    item = json.loads(line)
                except ValueError:
                    continue
                if item.get("target", {}).get("name") == "dev_fixture" and item.get("executable"):
                    fixtures.append(item["executable"])
            if len(set(fixtures)) != 1:
                raise ValueError("Cargo did not identify the actual PR fixture executable")
            fixture = Path(fixtures[0]).resolve(strict=True)
            record["fixture_sha256"] = hashlib.sha256(fixture.read_bytes()).hexdigest()
            step("build-plugin", [
                sys.executable, str(project / "scripts/package_plugin.py"),
                "--output", str(output / "built-package"),
            ])
            package_command = [
                sys.executable, str(ROOT / "scripts/package_scenarios.py"),
                "--project", str(project), "--package", str(output / "built-package"),
                "--fixture", str(fixture), "--output", str(output / "package-checks"),
            ]
            if "connection" in strategy["risks"]:
                package_command.append("--connection")
            step("package-scenarios", package_command)
            scenarios = json.loads((output / "package-checks/SCENARIOS.json").read_text())
            manifest = scenarios["manifest"]
            if (scenarios["passed"] is not True or manifest["source_commit"] != source["commit"]
                    or manifest["build_sha256"] != source["fingerprint"]
                    or scenarios["fixture_sha256"] != record["fixture_sha256"]):
                raise ValueError("The actual package or fixture differs from the tested PR")
            record["package_checks_sha256"] = hashlib.sha256(
                (output / "package-checks/SCENARIOS.json").read_bytes()).hexdigest()
        step("related-tests", [
            sys.executable, str(ROOT / "scripts/verify_pr_tests.py"),
            "--plan", str(output / "PLAN.json"), "--report", str(output / "TESTS.json"),
        ])
        tests = json.loads((output / "TESTS.json").read_text())
        bindings = json.loads((project / "acceptance/bindings.json").read_text())
        pr_plan.validate_report(tests, strategy, source, bound_inventory(bindings, sys.platform))
        record["tests_sha256"] = hashlib.sha256((output / "TESTS.json").read_bytes()).hexdigest()
        if (identity(project) != source or identity() != record["checker"]
                or git("status", "--porcelain", "--untracked-files=all", root=project)):
            raise ValueError("PR source changed during delivery checks")
        record.update(passed=True, acceptance_status="not_performed")
        return 0
    except (Exception, KeyboardInterrupt) as error:
        record["error"] = {"type": type(error).__name__, "detail": str(error)}
        print(str(error), file=sys.stderr)
        return 1
    finally:
        record.update(seconds=time.monotonic() - started,
                      finished_at=datetime.now(timezone.utc).isoformat())
        save(report_path, record)
        print(json.dumps({"scope": "pr", "passed": record["passed"],
                          "seconds": record["seconds"], "report": str(report_path)}))
