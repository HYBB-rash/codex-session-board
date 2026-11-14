import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class ValidationCliTests(unittest.TestCase):
    def invoke(self, *arguments, environment=None):
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/validation_bootstrap.py"), *map(str, arguments)],
            env=environment or dict(os.environ, CSB_VALIDATION_PYTHON=sys.executable),
            cwd=ROOT, text=True, capture_output=True, timeout=60)

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
            output = Path(temporary) / "preflight"
            result = self.invoke("--scope", "preflight", "--output", output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            record = json.loads((output / "VALIDATION.json").read_text())
            self.assertTrue(record["scope_passed"])
            self.assertFalse(record["release_ready"])
            self.assertTrue(record["cleanup_confirmed"])
            self.assertTrue((output / "source-package/SOURCE.tar.gz").is_file())
            read = self.invoke("--report", output)
            self.assertEqual(read.returncode, 0, read.stdout + read.stderr)
            (output / "preparation.log").write_text("modified preparation")
            rejected = self.invoke("--report", output)
            self.assertEqual(rejected.returncode, 3)
            self.assertIn("摘要不匹配", rejected.stderr)
