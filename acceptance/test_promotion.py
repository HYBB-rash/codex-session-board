import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from scripts import promote_candidate as promotion
from scripts import validation_report


class PromotionTests(unittest.TestCase):
    def test_main_lease_rejects_an_advanced_base_and_never_rewinds_history(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, remote = root / "source", root / "remote.git"
            subprocess.run(["git", "init", "--bare", "-q", str(remote)], check=True)
            subprocess.run(["git", "init", "-qb", "main", str(source)], check=True)
            def command(*arguments):
                return subprocess.check_output(["git", *arguments], cwd=source, text=True,
                                               stderr=subprocess.STDOUT).strip()
            for name, value in (("user.name", "Lease fixture"), ("user.email", "fixture@example.invalid"),
                                ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")):
                command("config", name, value)
            revisions = []
            for value in ("base", "intermediate", "candidate"):
                (source / "README.md").write_text(value)
                command("add", ".")
                command("commit", "-qm", value)
                revisions.append(command("rev-parse", "HEAD"))
            base, intermediate, candidate = revisions
            command("remote", "add", "origin", str(remote))
            command("push", "origin", f"{base}:refs/heads/main")
            command("push", "origin", f"{intermediate}:refs/heads/main")
            with patch.object(promotion, "ROOT", source):
                with self.assertRaises(subprocess.CalledProcessError):
                    promotion.advance(candidate, base, "refs/heads/main")
                self.assertTrue(command("ls-remote", "origin", "refs/heads/main").startswith(intermediate))
                with self.assertRaises(subprocess.CalledProcessError):
                    promotion.advance(base, intermediate, "refs/heads/main")
                self.assertTrue(command("ls-remote", "origin", "refs/heads/main").startswith(intermediate))
                promotion.advance(candidate, intermediate, "refs/heads/main")
            self.assertTrue(command("ls-remote", "origin", "refs/heads/main").startswith(candidate))

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

    def test_pr_ci_is_checked_again_after_artifacts_and_before_the_main_push(self):
        from scripts import pr_queue_gates
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(promotion, "identity", return_value=self.source), \
                    patch.object(promotion, "git", return_value=""), \
                    patch.object(promotion, "github", side_effect=[json.dumps(self.run), json.dumps(self.jobs)]), \
                    patch.object(promotion, "collect_platform", side_effect=lambda *args: (args[2], {}, {})) as collect, \
                    patch.object(promotion.subprocess, "run") as run, \
                    patch.object(pr_queue_gates, "verify", return_value=None), \
                    patch.object(promotion, "require_remote_target") as remote, \
                    self.assertRaisesRegex(ValueError, "PR checks are pending"):
                promotion.promote(self.source["commit"], "base", 1, "owner/repo",
                                  Path(temporary) / "receipt.json", pr_gates={"items": [7]})
            self.assertEqual(collect.call_count, 2)
            remote.assert_not_called()
            self.assertFalse(any(command.args[0][:2] == ["git", "push"] for command in run.call_args_list))

    def test_context_promotion_requires_exact_exclusive_evidence_and_never_rebuilds_packages(self):
        from scripts import validate_context
        required = {"Classify integration", "Validate context records", "Candidate gate"}
        jobs = {"jobs": [{"name": name, "status": "completed", "conclusion": "success"} for name in required]
                + [{"name": "Finalize candidate", "status": "completed", "conclusion": "skipped"}]}
        expected = {"scope": "context", "context_only": True, "base": "base",
                    "source_commit": self.source["commit"], "reference": "refs/heads/main",
                    "implementation_unchanged": True}
        for change in ({}, {"base": "stale"}, {"source_commit": "other"},
                       {"implementation_unchanged": False}):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                def github(*arguments):
                    if arguments[:2] == ("run", "download"):
                        directory = Path(arguments[arguments.index("--dir") + 1])
                        (directory / "CONTEXT.json").write_text(json.dumps({**expected, **change}))
                        return ""
                    return json.dumps(jobs if "/jobs?" in arguments[1] else self.run)
                with patch.object(promotion, "identity", return_value=self.source), \
                        patch.object(promotion, "git", return_value=""), \
                        patch.object(validate_context, "context_only", return_value=True), \
                        patch.object(promotion, "github", side_effect=github), \
                        patch.object(promotion, "require_remote_target", side_effect=["base", self.source["commit"]]), \
                        patch.object(promotion.subprocess, "run") as run, \
                        patch.object(promotion, "collect_platform") as collect:
                    output = Path(temporary) / "receipt.json"
                    if change:
                        with self.assertRaises(ValueError):
                            promotion.promote_context(self.source["commit"], "base", 1, "owner/repo", output)
                        run.assert_not_called()
                    else:
                        promotion.promote_context(self.source["commit"], "base", 1, "owner/repo", output)
                        self.assertEqual(json.loads(output.read_text())["scope"], "context")
                        self.assertEqual(run.call_args.args[0][:2], ["git", "push"])
                    collect.assert_not_called()

    def artifact(self, directory):
        report = {
            "automated_passed": True, "build_sha256": self.source["fingerprint"],
            "scope": "automated", "passed": True, "release_ready": True,
            "tests": [{"status": "pass"}], "requirements": [{"status": "pass"}],
        }
        (directory / "automated.json").write_text(json.dumps(report))
        (directory / "STARTUP.json").write_text(json.dumps({
            "scope": "package_startup", "passed": True, "cleanup_confirmed": True,
            "checks": [], "source_commit": self.source["commit"],
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
