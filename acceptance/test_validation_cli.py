import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]


class ValidationCliTests(unittest.TestCase):
    def invoke(self, *arguments, environment=None, root=ROOT):
        return subprocess.run(
            [sys.executable, str(root / "scripts/validation_bootstrap.py"), *map(str, arguments)],
            env=environment or dict(os.environ, CSB_VALIDATION_PYTHON=sys.executable),
            cwd=root, text=True, capture_output=True, timeout=60)

    def committed_source(self, parent):
        from scripts.package_migration import build_package
        archive = parent / "source.tar.gz"
        build_package(ROOT, archive)
        with tarfile.open(archive) as source:
            source.extractall(parent, filter="data")
        root = parent / "codex-session-board"
        commands = [
            ["init", "--quiet"],
            ["config", "user.name", "Fixture"],
            ["config", "user.email", "fixture@example.invalid"],
            ["add", "."],
            ["-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
             "commit", "--quiet", "-m", "fixture"],
        ]
        for arguments in commands:
            subprocess.run(["git", *arguments], cwd=root, check=True,
                           capture_output=True, timeout=10)
        return root

    def test_plan_and_legacy_report_are_read_only_and_do_not_require_environment_setup(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "uncreated"
            plan = self.invoke("--scope", "preflight", "--plan", "--output", output)
            self.assertEqual(plan.returncode, 0, plan.stderr)
            self.assertFalse(json.loads(plan.stdout)["executed"])
            self.assertFalse(output.exists())
            output.mkdir()
            path = output / "VALIDATION.json"
            path.write_text(json.dumps({"schema_version": 1, "scope": "candidate",
                                        "passed": True, "finished_at": "historical"}))
            before = path.read_bytes()
            result = self.invoke("--report", output, environment=dict(
                os.environ, CSB_VALIDATION_PYTHON="/missing/python"))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("不能用于新候选晋升", result.stdout)
            self.assertEqual(path.read_bytes(), before)

    def test_missing_git_persists_an_initialization_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed"
            result = self.invoke("--scope", "preflight", "--output", output,
                                 environment=dict(os.environ, PATH=temporary,
                                                  CSB_VALIDATION_PYTHON=sys.executable))
            self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
            record = json.loads((output / "VALIDATION.json").read_text())
            self.assertFalse(record["passed"])
            self.assertEqual(record["preparation"]["status"], "fail")
            self.assertIn("git", record["first_failure"]["message"])
            self.assertTrue(record["cleanup_confirmed"])

    def test_preflight_and_report_reader_verify_actual_source_package_and_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = self.committed_source(Path(temporary))
            output = Path(temporary) / "preflight"
            result = self.invoke("--scope", "preflight", "--output", output, root=root)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            record = json.loads((output / "VALIDATION.json").read_text())
            self.assertTrue(record["scope_passed"])
            self.assertFalse(record["release_ready"])
            self.assertTrue(record["cleanup_confirmed"])
            self.assertTrue((output / "source-package/SOURCE.tar.gz").is_file())
            read = self.invoke("--report", output, root=root)
            self.assertEqual(read.returncode, 0, read.stdout + read.stderr)
            (output / "preparation.log").write_text("modified preparation")
            rejected = self.invoke("--report", output, root=root)
            self.assertEqual(rejected.returncode, 3)
            self.assertIn("摘要不匹配", rejected.stderr)

    @patch.dict(os.environ, {
        "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "user.useConfigOnly",
        "GIT_CONFIG_VALUE_0": "true",
    })
    def test_pr_bootstrap_preserves_foreign_source_and_rejects_wrong_head_once(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = self.committed_source(Path(temporary))
            checker_parent = Path(temporary) / "checker"
            checker_parent.mkdir()
            checker = self.committed_source(checker_parent)
            output = Path(temporary) / "pr"
            subprocess.run(["git", "-c", "commit.gpgsign=false", "commit", "--allow-empty",
                            "-m", "second source snapshot"], cwd=root, check=True,
                           capture_output=True)
            result = self.invoke("--scope", "pr", "--project", root, "--base", "HEAD",
                                 "--expect-head", "HEAD~1", "--risk", "connection",
                                 "--output", output, root=checker)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            record = json.loads((output / "VALIDATION.json").read_text())
            self.assertEqual(record["scope"], "pr")
            self.assertFalse(record["passed"])
            self.assertEqual(record["project"], str(root.resolve()))
            self.assertIn("checker", record)
            self.assertEqual(record["steps"], [])
            self.assertIn("requested PR head", record["error"]["detail"])
            before = (output / "VALIDATION.json").read_bytes()
            read = self.invoke("--report", output, root=checker)
            self.assertEqual(read.returncode, 0, read.stdout + read.stderr)
            self.assertIn(record["error"]["detail"], read.stdout)
            self.assertEqual(before, (output / "VALIDATION.json").read_bytes())

    def test_pr_plan_uses_risk_selection_without_creating_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = self.committed_source(Path(temporary))
            output = Path(temporary) / "unused"
            result = self.invoke("--scope", "pr", "--project", root, "--base", "HEAD",
                                 "--risk", "connection", "--plan", "--output", output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            record = json.loads(result.stdout)
            self.assertFalse(record["executed"])
            self.assertTrue(record["plan"]["package_required"])
            self.assertFalse(output.exists())
            result = self.invoke("--scope", "pr", "--project", root, "--base", "HEAD",
                                 "--stage", "submit", "--plan", "--output", output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(json.loads(result.stdout)["plan"]["stage"], "submit")
            self.assertFalse(output.exists())

    def test_pr_only_options_and_unsupported_modes_are_rejected_before_initialization(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "unused"
            for options in (("--scope", "candidate", "--project", ROOT),
                            ("--scope", "candidate", "--stage", "submit"),
                            ("--staged", "--expect-head", "HEAD"),
                            ("--scope", "pr", "--timeout", "5"),
                            ("--scope", "pr", "--integration-base", "HEAD"),
                            ("--scope", "pr", "--initialized-run", "foreign"),
                            ("--scope", "pr", "--staged")):
                with self.subTest(options=options):
                    result = self.invoke(*options, "--output", output)
                    self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                    self.assertFalse(output.exists())
