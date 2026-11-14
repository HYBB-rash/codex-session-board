import hashlib
import json
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
from datetime import datetime, timezone
from pathlib import Path

from scripts.package_migration import ROOT, build_package
from scripts.verify_acceptance import (
    digest, execute_adapter, fingerprint, validate_contract, validate_report_path,
    validate_results, verify, requirement_tests,
)


class GateTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.contract_path = self.root / "acceptance/requirements.json"
        self.bindings_path = self.root / "acceptance/bindings.json"
        self.contract_path.parent.mkdir()
        self.contract = {
            "schema_version": 1, "contract_id": "fixture",
            "requirements": [
                {
                    "id": identifier, "title": identifier, "given": "fixture",
                    "when": "verify", "then": ["observable fixture result"],
                    "verification": kind,
                }
                for identifier, kind in (("AUTO", "automated"), ("LIVE", "live"))
            ],
        }
        self.driver = (
            "import json,sys; request=json.load(sys.stdin); "
            "print(json.dumps({'protocol':1,'results':["
            "{'id':identifier,'status':'pass'} for identifier in request['tests']]}))"
        )
        self.bindings = {
            "schema_version": 1,
            "adapter": {"command": [sys.executable, "-c", self.driver]},
            "fingerprint_paths": ["source.txt", "acceptance"],
            "requirements": {"AUTO": ["fixture.test"], "LIVE": []},
        }
        self.contract_path.write_text(json.dumps(self.contract))
        self.bindings_path.write_text(json.dumps(self.bindings))
        (self.root / "source.txt").write_text("implementation one")
        (self.root / "acceptance/model.py").write_text("reference model")
        lock = {
            "files": {
                name: digest(self.contract_path.parent / name)
                for name in ("requirements.json", "model.py")
            }
        }
        self.contract_path.with_name("contract.lock.json").write_text(json.dumps(lock))

    def tearDown(self):
        self.directory.cleanup()

    def run_gate(self, scope="release", evidence=None):
        return verify(self.root, self.contract_path, self.bindings_path, scope, evidence)

    def test_platform_branches_select_real_tests_and_keep_common_checks(self):
        mapping = {"common": ["shared"], "linux": ["proc"], "darwin": ["rejection"]}
        for platform, expected in (("linux", ["shared", "proc"]),
                                   ("darwin", ["shared", "rejection"])):
            with self.subTest(platform=platform), patch("sys.platform", platform):
                self.assertEqual(requirement_tests(mapping), expected)
                self.bindings["requirements"]["AUTO"] = mapping
                self.bindings_path.write_text(json.dumps(self.bindings))
                report, code = self.run_gate("automated")
                self.assertEqual(code, 0)
                self.assertEqual(report["platform"], platform)
                self.assertEqual(report["requirements"][0]["automated_tests"], expected)

    def test_platform_binding_errors_fail_closed(self):
        cases = [
            {"common": [], "linux": ["proc"]},
            {"common": [], "linux": ["proc"], "darwin": ["duplicate", "duplicate"]},
            {"common": ["shared"], "linux": ["shared"], "darwin": []},
        ]
        for mapping in cases:
            with self.subTest(mapping=mapping), patch("sys.platform", "linux"):
                with self.assertRaises(ValueError):
                    requirement_tests(mapping)
        self.bindings["requirements"]["AUTO"] = {"common": [], "linux": ["proc"], "darwin": []}
        with patch("sys.platform", "linux"), self.assertRaises(ValueError):
            validate_contract(self.contract, self.bindings)
        with patch("sys.platform", "unknown"), self.assertRaises(ValueError):
            requirement_tests({"common": ["shared"], "linux": [], "darwin": []})

    def test_automatic_success_does_not_claim_release_completion(self):
        report, code = self.run_gate("automated")
        self.assertEqual(code, 0)
        self.assertTrue(report["automated_passed"])
        self.assertFalse(report["release_ready"])
        self.assertEqual(report["requirements"][1]["status"], "pending")
        report, code = self.run_gate()
        self.assertEqual(code, 2)
        self.assertFalse(report["passed"])

    def test_missing_or_duplicate_bindings_and_results_cannot_pass(self):
        for records in (
            [], [{"id": "wrong", "status": "pass"}],
            [{"id": "fixture.test", "status": "pass"}] * 2,
        ):
            with self.subTest(records=records), self.assertRaises(ValueError):
                validate_results({"protocol": 1, "results": records}, {"fixture.test"})
        self.bindings["requirements"]["AUTO"] = []
        with self.assertRaises(ValueError):
            validate_contract(self.contract, self.bindings)
        self.contract["requirements"].append(self.contract["requirements"][0])
        with self.assertRaises(ValueError):
            validate_contract(self.contract, self.bindings)

    def test_skipped_checks_and_nonzero_adapter_exit_cannot_pass(self):
        self.bindings["adapter"]["command"][-1] = self.driver.replace("'pass'", "'skip'")
        self.bindings_path.write_text(json.dumps(self.bindings))
        report, code = self.run_gate()
        self.assertEqual(code, 1)
        self.assertFalse(report["automated_passed"])
        self.bindings["adapter"]["command"][-1] = self.driver + "; sys.exit(1)"
        with self.assertRaises(ValueError):
            execute_adapter(self.root, self.bindings, ["fixture.test"])

    def test_missing_test_selector_is_an_error_instead_of_an_empty_pass(self):
        bindings = {
            "adapter": {
                "command": ["{python}", "acceptance/unittest_adapter.py"],
                "timeout_seconds": 10,
            },
        }
        with self.assertRaises(ValueError):
            execute_adapter(ROOT, bindings, ["acceptance.no_such_test"])

    def test_live_evidence_requires_current_build_and_valid_artifacts(self):
        artifact = self.root / "fixture-observation.txt"
        artifact.write_text("Synthetic fixture, not an actual desktop acceptance record.")
        evidence_path = self.root / "evidence.json"
        evidence = {
            "contract_sha256": digest(self.contract_path),
            "build_sha256": fingerprint(self.root, self.bindings["fingerprint_paths"]),
            "checks": {"LIVE": {
                "status": "pass", "operator": "test fixture",
                "environment": "synthetic test environment",
                "notes": "Only verifies the evidence validation mechanism.",
                "observed_at": datetime.now(timezone.utc).isoformat(),
                "artifacts": [{"path": artifact.name, "sha256": digest(artifact)}],
            }},
        }
        evidence_path.write_text(json.dumps(evidence))
        report, code = self.run_gate(evidence=evidence_path)
        self.assertEqual(code, 0)
        self.assertTrue(report["release_ready"])
        artifact.write_text("changed artifact")
        self.assertEqual(self.run_gate(evidence=evidence_path)[1], 2)
        artifact.write_text("Synthetic fixture, not an actual desktop acceptance record.")
        (self.root / "source.txt").write_text("implementation two")
        self.assertEqual(self.run_gate(evidence=evidence_path)[1], 2)

    def test_changed_contract_baseline_requires_an_explicit_update(self):
        self.contract["requirements"][0]["then"] = ["weakened behavior"]
        self.contract_path.write_text(json.dumps(self.contract))
        with self.assertRaises(ValueError):
            self.run_gate()

    def test_adapter_checks_the_target_instead_of_its_own_project(self):
        targets = [self.root / "project-a", self.root / "project-b"]
        for target, passes in zip(targets, (True, False)):
            target.mkdir()
            (target / "test_target.py").write_text(
                "import unittest\n"
                "class TargetTests(unittest.TestCase):\n"
                "    def test_implementation(self):\n"
                f"        self.assertTrue({passes!r})\n"
            )
        adapter = targets[0] / "acceptance/unittest_adapter.py"
        adapter.parent.mkdir()
        adapter.write_bytes((ROOT / "acceptance/unittest_adapter.py").read_bytes())
        bindings = {"adapter": {"command": [sys.executable, str(adapter)]}}
        identifier = "test_target.TargetTests.test_implementation"
        self.assertEqual(
            execute_adapter(targets[0], bindings, [identifier])[identifier]["status"],
            "pass",
        )
        self.assertEqual(
            execute_adapter(targets[1], bindings, [identifier])[identifier]["status"],
            "fail",
        )

    def test_report_cannot_overwrite_evidence_artifacts_or_verified_inputs(self):
        evidence = self.root / "evidence.json"
        artifact = self.root / "artifact.txt"
        artifact.write_text("protected proof")
        evidence.write_text(json.dumps({
            "checks": {"LIVE": {"artifacts": [{"path": artifact.name}]}},
        }))
        original = evidence.read_bytes()
        for output in (
            evidence, artifact, self.contract_path, self.bindings_path,
            self.root / "source.txt", self.root / "acceptance/new-report.json",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                validate_report_path(
                    self.root, self.contract_path, self.bindings_path, evidence, output
                )
        process = subprocess.run([
            sys.executable, str(ROOT / "scripts/verify_acceptance.py"),
            "--root", str(self.root), "--evidence", str(evidence),
            "--report", str(evidence),
        ], capture_output=True, text=True, timeout=10)
        self.assertNotEqual(process.returncode, 0)
        self.assertEqual(evidence.read_bytes(), original)
        self.assertEqual(artifact.read_text(), "protected proof")


class PackageTests(unittest.TestCase):
    def test_package_is_reproducible_and_has_only_portable_source(self):
        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "first.tar.gz"
            second = Path(directory) / "second.tar.gz"
            build_package(ROOT, first)
            build_package(ROOT, second)
            self.assertEqual(digest(first), digest(second))
            with tarfile.open(first) as archive:
                files = {
                    item.name.removeprefix("codex-session-board/"):
                    archive.extractfile(item).read()
                    for item in archive.getmembers()
                }
            manifest = json.loads(files.pop("PACKAGE-MANIFEST.json"))
            self.assertEqual(set(files), set(manifest["files"]))
            for name, body in files.items():
                self.assertFalse(Path(name).is_absolute())
                self.assertNotIn("..", Path(name).parts)
                self.assertFalse(any(part in name.split("/") for part in (
                    ".runtime", "__pycache__", "node_modules",
                )))
                self.assertNotIn(Path(name).suffix, {".db", ".sqlite3", ".log", ".png"})
                self.assertEqual(hashlib.sha256(body).hexdigest(), manifest["files"][name])
            self.assertIn("acceptance/requirements.json", files)
            self.assertIn("assets/app-bridge.js", files)
            self.assertNotIn("config.json", files)
            self.assertIn("src/store.rs", files)
            self.assertIn("src/main.rs", files)
            self.assertIn("Cargo.lock", files)
            self.assertIn("build.rs", files)
            for script in ROOT.glob("scripts/*.mjs"):
                self.assertIn(script.relative_to(ROOT).as_posix(), files)
            self.assertIn("scripts/rust_adapter.py", files)
            self.assertIn("Docs/MIGRATION.md", files)
            self.assertNotIn("main.py", files)
            self.assertNotIn("data", {Path(name).parts[0] for name in files})

    def test_package_requires_the_fingerprinted_build_gate_and_preserves_its_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            package = directory / "source.tar.gz"
            build_package(ROOT, package)
            with tarfile.open(package) as archive:
                archive.extractall(directory, filter="data")
            source = directory / "codex-session-board"
            manifest = tomllib.loads((source / "Cargo.toml").read_text())
            self.assertEqual(manifest["package"]["build"], "build.rs")
            patterns = json.loads((source / "acceptance/bindings.json").read_text())["fingerprint_paths"]
            self.assertIn("build.rs", patterns)
            before = fingerprint(source, patterns)
            gate = source / "build.rs"
            gate.write_bytes(gate.read_bytes() + b"\n")
            self.assertNotEqual(fingerprint(source, patterns), before)
            executable = directory / "build-gate"
            compiled = subprocess.run(
                ["rustc", str(gate), "-o", str(executable)],
                capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            accepted = subprocess.run(
                [str(executable)], cwd=source, capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            entry = source / "src/main.rs"
            original = entry.read_text()
            self.assertIn("mcp::serve(Some(path))", original)
            entry.write_text(original.replace("mcp::serve(Some(path))", "mcp::serve(None)"))
            rejected = subprocess.run(
                [str(executable)], cwd=source, capture_output=True, text=True, timeout=10,
            )
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("插件入口只能调用", rejected.stderr)
            entry.write_text(original)
            bypass = source / "src/connection_bypass.rs"
            for constructor in ("connect", "proxy", "remote"):
                bypass.write_text(f"fn bypass() {{ DesktopConnection::{constructor}(value); }}")
                rejected = subprocess.run(
                    [str(executable)], cwd=source, capture_output=True, text=True, timeout=10,
                )
                self.assertNotEqual(rejected.returncode, 0)
                self.assertIn("绕过统一客户端连接层", rejected.stderr)
            bypass.unlink()
            gate.unlink()
            with self.assertRaisesRegex(ValueError, "Missing a required migration file"):
                build_package(source, directory / "missing.tar.gz")
