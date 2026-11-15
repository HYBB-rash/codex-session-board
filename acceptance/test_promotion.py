import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from scripts import promote_candidate as promotion
from scripts import validation_report


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.source = {"commit": "a" * 40, "tree": "b" * 40, "fingerprint": "c" * 64}
        self.run = {
            "head_sha": self.source["commit"], "status": "completed", "conclusion": "success",
            "repository": {"full_name": "owner/repo"},
            "path": ".github/workflows/plugin-candidate.yml",
            "event": "push", "head_branch": "codex/integration-example",
        }
        self.jobs = {"jobs": [
            {"name": name, "status": "completed", "conclusion": "success"}
            for name in promotion.JOBS
        ]}

    def test_promotion_rejects_a_successful_run_for_another_commit(self):
        self.run["head_sha"] = "d" * 40
        with self.assertRaises(ValueError):
            promotion.validate_run(self.run, self.jobs, self.source, "owner/repo")

    def test_promotion_rejects_missing_failed_skipped_and_neutral_jobs(self):
        for conclusion in ("failure", "skipped", "neutral", "cancelled"):
            with self.subTest(conclusion=conclusion):
                self.jobs["jobs"][0]["conclusion"] = conclusion
                with self.assertRaises(ValueError):
                    promotion.validate_run(self.run, self.jobs, self.source, "owner/repo")
        self.jobs["jobs"].pop()
        with self.assertRaises(ValueError):
            promotion.validate_run(self.run, self.jobs, self.source, "owner/repo")

    def test_promotion_rejects_wrong_repository_workflow_or_branch(self):
        for field, value in (
            ("repository", {"full_name": "other/repo"}),
            ("path", ".github/workflows/pr-checks.yml"), ("head_branch", "main"),
        ):
            with self.subTest(field=field):
                run = {**self.run, field: value}
                with self.assertRaises(ValueError):
                    promotion.validate_run(run, self.jobs, self.source, "owner/repo")

    def artifact(self, directory):
        report = {
            "automated_passed": True, "build_sha256": self.source["fingerprint"],
            "scope": "automated", "passed": True, "release_ready": True,
            "tests": [{"status": "pass"}], "requirements": [{"status": "pass"}],
        }
        (directory / "automated.json").write_text(json.dumps(report))
        (directory / "STARTUP.json").write_text(json.dumps({
            "scope": "package_startup", "passed": True, "cleanup_confirmed": True,
            "checks": promotion.CHECKS, "source_commit": self.source["commit"],
            "build_sha256": self.source["fingerprint"], "binary_sha256": "e" * 64,
            "target": "aarch64-apple-darwin",
        }))
        for number in range(1, 4):
            (directory / f"step-{number}.log").write_text("validation transcript")
        record = {
            "scope": "candidate", "passed": True,
            "source": {**self.source, "platform": "darwin", "machine": "arm64"},
            "automated_sha256": hashlib.sha256((directory / "automated.json").read_bytes()).hexdigest(),
            "steps": [
                {"name": f"step-{number}", "returncode": 0,
                 "log_sha256": hashlib.sha256((directory / "step-1.log").read_bytes()).hexdigest()}
                for number in range(1, 4)
            ],
        }
        return self.complete_record(directory, record)

    def complete_record(self, directory, record, commands=None):
        commands = commands or [
            ["./scripts/check"],
            ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
            ["python3", "scripts/verify_acceptance.py", "--scope", "automated",
             "--report", str(directory / "automated.json")],
        ]
        environment = {
            "source_directory": "/candidate", "platform": "darwin", "machine": "arm64",
            "temporary_directory": "/private/tmp", "cargo_target": "/candidate/target",
            "cargo_owner": {"schema_version": 1, "source_directory": "/candidate",
                            "source_device": 1, "source_inode": 1},
            "tools": {name: {"path": f"/tools/{name}", "version": "fixture"}
                      for name in ("python3", "git", "cargo", "rustc", "node")},
        }
        record.update(schema_version=2, run_id="fixture", scope_passed=True, exit_code=0,
                      cleanup_confirmed=True, first_failure=None, finished_at="fixture",
                      environment=environment, selected_checks=commands, release_ready=True,
                      pending=[], acceptance_status="not_performed")
        receipts = {}
        for step, command in zip(record["steps"], commands):
            name = step["name"]
            path = directory / f"{name}.lease.json"
            path.write_text(json.dumps({
                "schema_version": 1, "run_id": "fixture", "name": name,
                "command": command, "cwd": "/candidate", "reason": "completed",
                "cleanup_confirmed": True, "command_returncode": 0, "group_started_at": "fixture",
                "group_pid": 1, "command_pid": 2, "supervisor_pid": 3, "owner_pid": 4}))
            (directory / f"{name}.stderr.log").write_text("diagnostic transcript")
            receipts[name] = validation_report.digest(path)
            step.update(arguments=command, seconds=1, cleanup_confirmed=True,
                        lease_sha256=receipts[name],
                        stderr_sha256=validation_report.digest(directory / f"{name}.stderr.log"))
        record["preparation"] = validation_report.phase(directory, "preparation", "pass", {
            "source": record["source"], "environment": environment, "selected_checks": commands})
        record["finalization"] = validation_report.phase(directory, "finalization", "pass", {
            "source_unchanged": True, "receipts": receipts})
        return record

    def test_promotion_rejects_legacy_incomplete_or_failed_new_phases(self):
        mutations = [
            lambda record: record.update(schema_version=1),
            lambda record: record.pop("preparation"),
            lambda record: record.pop("first_failure"),
            lambda record: record.update(first_failure={"message": "failed before retry"}),
            lambda record: record.update(cleanup_confirmed=False),
            lambda record: record["preparation"].update(status="fail"),
            lambda record: record["finalization"].update(status="fail"),
            lambda record: record["steps"].pop(),
            lambda record: record["steps"][0].update(cleanup_confirmed=False),
            lambda record: record["steps"][0].update(stderr_sha256="0" * 64),
        ]
        for number, mutate in enumerate(mutations):
            with self.subTest(number=number), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                record = self.artifact(directory)
                mutate(record)
                (directory / "VALIDATION.json").write_text(json.dumps(record))
                with self.assertRaises((ValueError, KeyError)):
                    promotion.validate_artifact(directory, self.source, "macos-arm64")

    def test_rehashed_receipts_cannot_hide_wrong_ownership_or_unconfirmed_cleanup(self):
        for field, value in (("run_id", "another-run"), ("cleanup_confirmed", False),
                             ("reason", "owner_lost"), ("command_returncode", 7),
                             ("command", ["true"]), ("group_started_at", "")):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                record = self.artifact(directory)
                path = directory / "step-1.lease.json"
                lease = json.loads(path.read_text())
                lease[field] = value
                path.write_text(json.dumps(lease))
                record["steps"][0]["lease_sha256"] = validation_report.digest(path)
                finalized = json.loads((directory / "finalization.log").read_text())
                finalized["receipts"]["step-1"] = validation_report.digest(path)
                record["finalization"] = validation_report.phase(
                    directory, "finalization", "pass", finalized)
                (directory / "VALIDATION.json").write_text(json.dumps(record))
                with self.assertRaises(ValueError):
                    promotion.validate_artifact(directory, self.source, "macos-arm64")

    def test_promotion_requires_both_platform_identity_and_unchanged_reports(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            record = self.artifact(directory)
            receipt = directory / "VALIDATION.json"
            receipt.write_text(json.dumps(record))
            promotion.validate_artifact(directory, self.source, "macos-arm64")
            with self.assertRaises(ValueError):
                promotion.validate_artifact(directory, self.source, "linux-x86_64")
            startup = directory / "STARTUP.json"
            original = json.loads(startup.read_text())
            for field, value in (
                ("cleanup_confirmed", False), ("passed", False), ("checks", []),
                ("source_commit", "d" * 40), ("build_sha256", "f" * 64),
                ("binary_sha256", "wrong"), ("target", "x86_64-unknown-linux-gnu"),
            ):
                startup.write_text(json.dumps({**original, field: value}))
                with self.subTest(field=field), self.assertRaises(ValueError):
                    promotion.validate_artifact(directory, self.source, "macos-arm64")
            startup.write_text(json.dumps(original))
            (directory / "automated.json").write_text("{}")
            with self.assertRaises(ValueError):
                promotion.validate_artifact(directory, self.source, "macos-arm64")

    def test_promotion_rejects_a_modified_validation_transcript(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            record = self.artifact(directory)
            (directory / "VALIDATION.json").write_text(json.dumps(record))
            (directory / "step-1.log").write_text("substituted transcript")
            with self.assertRaises(ValueError):
                promotion.validate_artifact(directory, self.source, "macos-arm64")

    def test_actual_package_must_be_immutable_and_match_the_started_binary(self):
        startup = {
            "source_commit": self.source["commit"], "build_sha256": self.source["fingerprint"],
            "binary_sha256": "e" * 64, "version": "0.3.80", "target": promotion.TARGETS["macos-arm64"],
        }
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            service = Mock()
            with patch("release_plugin.GitHub", return_value=service), \
                    patch("release_plugin.read_package") as read, \
                    patch("package_plugin.verify_package", return_value=startup) as verify:
                for release in (None, {"draft": True, "immutable": True},
                                {"draft": False, "immutable": False}):
                    service.release.return_value = release
                    with self.assertRaises(ValueError):
                        promotion.verify_saved_package(
                            "owner/repo", "macos-arm64", self.source, startup, directory / "invalid")
                read.assert_not_called()
                service.release.return_value = {"draft": False, "immutable": True}
                for key in startup:
                    verify.return_value = {**startup, key: "wrong"}
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        promotion.verify_saved_package(
                            "owner/repo", "macos-arm64", self.source, startup, directory / key)

    def test_scoped_promotion_recomputes_plan_and_rejects_omitted_or_failed_tests(self):
        from scripts.integration_plan import bound_inventory, selected
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            record = self.artifact(directory)
            strategy = {"profile": "ui", "base": "f" * 40, "paths": ["index.html"], "replay": None}
            bindings = json.loads((promotion.ROOT / "acceptance/bindings.json").read_text())
            inventory = bound_inventory(bindings, "darwin")
            chosen = selected(inventory, "ui")
            report = {
                "scope": "integration", "plan": strategy, "source": record["source"],
                "passed": True, "release_ready": False, "inventory": sorted(inventory),
                "selected": sorted(chosen), "not_run": sorted(inventory - chosen),
                "tests": [{"id": name, "status": "pass"} for name in sorted(chosen)],
            }
            record.pop("automated_sha256")
            record["plan"] = strategy
            path = directory / "integration.json"
            commands = record["selected_checks"][:2] + [[
                "python3", "scripts/verify_integration.py", "--base", strategy["base"],
                "--report", str(path)]]
            self.complete_record(directory, record, commands)
            record.update(release_ready=False, pending=["完整自动验收", "目标环境验收"],
                          not_run=report["not_run"])
            with patch("scripts.integration_plan.plan", return_value=strategy):
                for change in ({}, {"tests": report["tests"][:-1]}, {"not_run": []},
                               {"tests": [{**test, "status": "fail"} for test in report["tests"]]}):
                    path.write_text(json.dumps({**report, **change}))
                    record["integration_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
                    (directory / "VALIDATION.json").write_text(json.dumps(record))
                    if change:
                        with self.subTest(change=next(iter(change))), self.assertRaises(ValueError):
                            promotion.validate_artifact(directory, self.source, "macos-arm64", strategy["base"])
                    else:
                        promotion.validate_artifact(directory, self.source, "macos-arm64", strategy["base"])
                        with self.assertRaises(ValueError):
                            promotion.validate_artifact(directory, self.source, "macos-arm64")
            with patch("scripts.integration_plan.plan", return_value={**strategy, "profile": "full"}):
                with self.assertRaises(ValueError):
                    promotion.validate_artifact(directory, self.source, "macos-arm64", strategy["base"])
