import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from scripts import promote_candidate as promotion


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
        return {
            "scope": "candidate", "passed": True,
            "source": {**self.source, "platform": "darwin", "machine": "arm64"},
            "automated_sha256": hashlib.sha256((directory / "automated.json").read_bytes()).hexdigest(),
            "steps": [
                {"name": f"step-{number}", "returncode": 0,
                 "log_sha256": hashlib.sha256((directory / "step-1.log").read_bytes()).hexdigest()}
                for number in range(1, 4)
            ],
        }

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
