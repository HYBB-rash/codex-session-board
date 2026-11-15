from argparse import Namespace
from contextlib import chdir
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from scripts import integrate_candidate as integrate
from scripts import promote_candidate as promotion
from scripts import validate_context as context


class RehearsalTargetTests(unittest.TestCase):
    reference = "refs/heads/codex/rehearsal-main-fixture"
    branch = "codex/integration-rehearsal-fixture"

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.root = self.directory / "source"
        self.root.mkdir()
        self.enterContext(chdir(self.root))
        subprocess.run(["git", "init", "--quiet"], check=True)
        context.git("config", "user.name", "Fixture")
        context.git("config", "user.email", "fixture@example.invalid")
        context.git("config", "commit.gpgsign", "false")
        context.git("config", "core.hooksPath", "/dev/null")
        Path("runtime.rs").write_text("original\n")
        self.commit()
        self.base = context.git("rev-parse", "HEAD")
        subprocess.run(["git", "init", "--bare", "--quiet", str(self.directory / "remote")], check=True)
        context.git("remote", "add", "origin", str(self.directory / "remote"))
        context.git("push", "--quiet", "origin", f"{self.base}:{self.reference}")
        Path("context").mkdir()
        self.write_configuration(self.reference, self.base)
        self.commit()

    def commit(self):
        context.git("add", ".")
        context.git("commit", "--quiet", "-m", "fixture")

    def write_configuration(self, reference, base):
        Path("context/integration-target.json").write_text(json.dumps({
            "reference": reference, "base": base,
        }))

    def test_normal_target_never_reads_rehearsal_configuration(self):
        command = Mock(side_effect=AssertionError("Must not read target configuration"))
        context.require_target("refs/heads/main", self.base, "codex/integration-normal",
                               git_command=command)
        command.assert_not_called()
        self.assertEqual(context.classification_target("codex/integration-normal", self.base),
                         (self.base, "refs/heads/main"))
        with self.assertRaises(ValueError):
            context.require_target("refs/heads/main", self.base, self.branch)

    def test_rehearsal_classification_uses_fixed_base_and_requires_full_candidate_path(self):
        report = self.directory / "classification.json"
        output = self.directory / "outputs"
        with patch("sys.argv", ["classify", "--branch", self.branch, "--output", str(report),
                                "--github-output", str(output)]), \
                patch.object(context, "context_only", side_effect=AssertionError("No shortcut")):
            context.main()
        record = json.loads(report.read_text())
        self.assertEqual(record["base"], self.base)
        self.assertEqual(record["reference"], self.reference)
        self.assertFalse(record["context_only"])
        self.assertIn("context_only=false", output.read_text())

    def test_target_rejects_wrong_branch_reference_base_and_configuration(self):
        for reference, base, branch in (
            (self.reference, self.base, "codex/integration-normal"),
            ("refs/heads/main-other", self.base, self.branch),
            ("refs/heads/codex/rehearsal-main-../main", self.base, self.branch),
            (self.reference, "HEAD", self.branch),
            (self.reference + "-other", self.base, self.branch),
            (self.reference, "a" * 40, self.branch),
        ):
            with self.subTest(reference=reference, base=base, branch=branch):
                with self.assertRaises(ValueError):
                    context.require_target(reference, base, branch)

    def test_classification_rejects_advanced_or_missing_remote_target(self):
        context.git("push", "--quiet", "origin", f"HEAD:{self.reference}")
        with self.assertRaises(ValueError):
            context.classification_target(self.branch, self.base)
        context.git("push", "--quiet", "origin", f":{self.reference}")
        with self.assertRaises(ValueError):
            context.classification_target(self.branch, self.base)

    def test_target_rejects_a_fixed_base_outside_candidate_ancestry(self):
        unrelated = context.git("commit-tree", "HEAD^{tree}", "-m", "unrelated")
        self.write_configuration(self.reference, unrelated)
        self.commit()
        with self.assertRaises(subprocess.CalledProcessError):
            context.require_target(self.reference, unrelated, self.branch)

    def test_stale_lease_cannot_overwrite_a_concurrently_advanced_target(self):
        candidate = context.git("rev-parse", "HEAD")
        context.require_remote_target(self.reference, (self.base,))
        sibling = context.git("commit-tree", "HEAD^{tree}", "-p", self.base, "-m", "concurrent")
        context.git("push", "--quiet", "origin", f"{sibling}:{self.reference}")
        result = subprocess.run(
            ["git", "push", f"--force-with-lease={self.reference}:{self.base}", "origin",
             f"{candidate}:{self.reference}"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(context.require_remote_target(self.reference, (sibling,)), sibling)
        self.assertEqual(context.git("ls-remote", "origin", "refs/heads/main"), "")

    def test_resume_requires_explicit_matching_rehearsal_reference(self):
        record = self.directory / "record"
        record.mkdir()
        integrate.save(record / "integration.json", {"reference": self.reference})
        with patch.object(integrate, "continue_candidate") as proceed:
            with self.assertRaises(ValueError):
                integrate.start(Namespace(output=record, resume=True, timeout=1,
                                          reference="refs/heads/main"))
            proceed.assert_not_called()

    def test_promotion_keeps_both_platform_checks_and_targets_only_the_rehearsal_ref(self):
        source = {"commit": context.git("rev-parse", "HEAD")}
        run = {"head_branch": self.branch}
        output = self.directory / "promotion.json"
        checks = []
        original_run = subprocess.run

        def collect(repository, run_id, label, observed, base, directory):
            checks.append((label, observed, base))
            return label, {"validated": True}, {"package": True}

        def command(arguments, **options):
            if arguments[:2] == ["git", "push"]:
                self.assertIn(f"--force-with-lease={self.reference}:{self.base}", arguments)
                self.assertEqual(arguments[-1], f"{source['commit']}:{self.reference}")
            return original_run(arguments, **options)

        with patch.object(promotion, "ROOT", self.root), \
                patch.object(promotion, "identity", return_value=source), \
                patch.object(promotion, "git", side_effect=context.git), \
                patch.object(promotion, "github", side_effect=[json.dumps(run), "{}"]), \
                patch.object(promotion, "validate_run"), \
                patch.object(promotion, "collect_platform", side_effect=collect), \
                patch.object(promotion.subprocess, "run", side_effect=command):
            receipt = promotion.promote(source["commit"], self.base, 1, "fixture/repo",
                                        output, self.reference)
        self.assertEqual({entry[0] for entry in checks}, set(promotion.PLATFORMS))
        self.assertTrue(all(entry[1:] == (source, self.base) for entry in checks))
        self.assertEqual(receipt["reference"], self.reference)
        self.assertEqual(context.require_remote_target(self.reference, (source["commit"],)),
                         source["commit"])
        self.assertEqual(context.git("ls-remote", "origin", "refs/heads/main"), "")
