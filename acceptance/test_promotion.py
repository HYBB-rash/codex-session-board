import hashlib
import json
from pathlib import Path
import tempfile
import unittest

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
        report = {"automated_passed": True, "build_sha256": self.source["fingerprint"]}
        (directory / "automated.json").write_text(json.dumps(report))
        (directory / "step-1.log").write_text("validation transcript")
        return {
            "scope": "candidate", "passed": True,
            "source": {**self.source, "platform": "darwin", "machine": "arm64"},
            "automated_sha256": hashlib.sha256((directory / "automated.json").read_bytes()).hexdigest(),
            "steps": [
                {"name": "step-1", "returncode": 0,
                 "log_sha256": hashlib.sha256((directory / "step-1.log").read_bytes()).hexdigest()}
                for _ in range(3)
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
