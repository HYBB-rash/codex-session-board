import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts import validation_environment as environment
from scripts import validation_report as report


class ValidationEnvironmentTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        self.source = self.root / "source"
        self.source.mkdir()

    def test_explicit_interpreter_is_used_and_invalid_selection_never_falls_back(self):
        selected = environment.choose_python(self.source, dict(os.environ,
                                            CSB_VALIDATION_PYTHON=sys.executable))
        self.assertEqual(selected["path"], os.path.abspath(sys.executable))
        for value in (str(self.root / "missing"), "/usr/bin/false"):
            with self.subTest(value=value), self.assertRaisesRegex(RuntimeError, "CSB_VALIDATION_PYTHON"):
                environment.choose_python(self.source, dict(os.environ, CSB_VALIDATION_PYTHON=value))

    def test_owned_cache_accepts_the_same_directory_and_rejects_another_copy(self):
        target = self.root / "cache"
        self.assertEqual(environment.claim_cache(self.source, target), str(target))
        alias = self.root / "alias"
        alias.symlink_to(self.source, target_is_directory=True)
        self.assertEqual(environment.claim_cache(alias, target), str(target))
        before = (target / ".validation-owner.json").read_bytes()
        (self.root / "other-copy").mkdir()
        with self.assertRaisesRegex(RuntimeError, "其他源码"):
            environment.claim_cache(self.root / "other-copy", target)
        self.assertEqual((target / ".validation-owner.json").read_bytes(), before)
        previous_target = environment.default_target(self.source)
        self.source.rename(self.root / "original-copy")
        self.source.mkdir()
        self.assertNotEqual(environment.default_target(self.source), previous_target)
        with self.assertRaisesRegex(RuntimeError, "其他源码"):
            environment.claim_cache(self.source, target)

    def test_unowned_nonempty_cache_is_never_silently_adopted(self):
        target = self.root / "cache"
        target.mkdir()
        artifact = target / "old-binary"
        artifact.write_bytes(b"from another checkout")
        with self.assertRaisesRegex(RuntimeError, "没有归属"):
            environment.claim_cache(self.source, target)
        self.assertEqual(artifact.read_bytes(), b"from another checkout")
        self.assertFalse((target / ".validation-owner.json").exists())

    def test_preflight_resolves_python_and_canonical_temporary_directory_without_rust(self):
        alias = self.root / "temporary-alias"
        alias.symlink_to(self.root, target_is_directory=True)
        output = self.root / "result"
        report.create(output, self.source, "preflight")
        with patch.dict(os.environ, {"TMPDIR": str(alias), "CSB_VALIDATION_PYTHON": sys.executable}):
            child, metadata = environment.resolve(self.source, output, "preflight")
        expected = "/private/tmp" if sys.platform == "darwin" else str(self.root)
        self.assertEqual(child["TMPDIR"], expected)
        self.assertEqual(set(metadata["tools"]), {"python3", "git"})
        self.assertIsNone(metadata["cargo_target"])
        actual = subprocess.check_output(["python3", "-c", "import sys;print(sys.executable)"],
                                         env=child, text=True).strip()
        self.assertEqual(actual, sys.executable)

    def test_initial_record_is_failed_closed_and_existing_output_is_preserved(self):
        output = self.root / "result"
        created = report.create(output, self.source, "candidate")
        saved = json.loads((output / "VALIDATION.json").read_text())
        self.assertEqual(created, saved)
        self.assertFalse(saved["passed"])
        self.assertFalse(saved["cleanup_confirmed"])
        with self.assertRaises(FileExistsError):
            report.create(output, self.source, "candidate")
        report.failure(created, "first", 1)
        report.failure(created, "cleanup", 3)
        self.assertEqual(created["first_failure"]["message"], "first")
        self.assertFalse(created["scope_passed"])

    def test_bootstrap_persists_an_explicit_interpreter_failure_before_project_initialization(self):
        output = self.root / "broken-environment"
        script = Path(__file__).resolve().parents[1] / "scripts/validation_bootstrap.py"
        result = subprocess.run([sys.executable, str(script), "--output", str(output)],
                                env=dict(os.environ, CSB_VALIDATION_PYTHON=str(self.root / "missing")),
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        saved = json.loads((output / "VALIDATION.json").read_text())
        self.assertEqual(saved["preparation"]["status"], "fail")
        self.assertIsNone(saved["source"])
        self.assertIn("CSB_VALIDATION_PYTHON", saved["first_failure"]["message"])
        self.assertFalse(saved["passed"])
        self.assertTrue(saved["cleanup_confirmed"])
