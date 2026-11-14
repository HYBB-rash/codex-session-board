from contextlib import chdir
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import validate_candidate as candidate
from scripts import validate_context


class CandidateGateTests(unittest.TestCase):
    def run_candidate(self, *, changed=False, dirty=False, mismatch=False, scoped=False, omitted=False):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        parent = Path(directory.name)
        root = parent / "source"
        root.mkdir()
        (root / "acceptance").mkdir()
        (root / "acceptance/bindings.json").write_bytes(
            (candidate.ROOT / "acceptance/bindings.json").read_bytes())
        output = parent / "result"
        identity = {"commit": "a" * 40, "tree": "b" * 40, "fingerprint": "c" * 64}
        after = {**identity, **({"commit": "d" * 40} if changed else {})}
        strategy = {"profile": "ui", "base": "f" * 40, "paths": ["index.html"], "replay": None}

        def execute(name, arguments, destination, **options):
            (destination / f"{name}.log").write_text("actual step transcript\n")
            if "--report" in arguments:
                report = {
                    "automated_passed": True,
                    "build_sha256": "e" * 64 if mismatch else identity["fingerprint"],
                    "scope": "automated", "passed": True, "release_ready": True,
                    "tests": [{"status": "pass"}], "requirements": [{"status": "pass"}],
                }
                report_name = "automated"
                if scoped:
                    from scripts.integration_plan import bound_inventory, selected
                    bindings = json.loads((root / "acceptance/bindings.json").read_text())
                    inventory = bound_inventory(bindings, candidate.sys.platform)
                    chosen = selected(inventory, "ui")
                    report = {
                        "scope": "integration", "plan": strategy, "source": identity,
                        "passed": True, "release_ready": False, "inventory": sorted(inventory),
                        "selected": sorted(chosen), "not_run": sorted(inventory - chosen),
                        "tests": [{"id": item, "status": "pass"} for item in sorted(chosen)],
                    }
                    if omitted:
                        report["tests"].pop()
                    report_name = "integration"
                (destination / f"{report_name}.json").write_text(json.dumps(report))
            (destination / f"{name}.request.json").write_text("{}")
            (destination / f"{name}.lease.json").write_text(json.dumps({
                "run_id": options["run_id"], "name": name, "cleanup_confirmed": True}))
            return {"name": name, "arguments": arguments, "returncode": 0, "seconds": 1,
                    "log_sha256": hashlib.sha256((destination / f"{name}.log").read_bytes()).hexdigest(),
                    "cleanup_confirmed": True}

        arguments = ["validate", "--output", str(output)]
        if scoped:
            arguments.extend(["--integration-base", strategy["base"]])
        with mock.patch.object(candidate, "ROOT", root), \
                mock.patch.object(candidate, "identity", side_effect=[identity, after]), \
                mock.patch.object(candidate, "git", return_value="dirty" if dirty else ""), \
                mock.patch.object(candidate.run.processes, "execute", side_effect=execute), \
                mock.patch("scripts.integration_plan.plan", return_value=strategy), \
                mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": str(root / "target")}), \
                mock.patch.object(candidate.sys, "argv", arguments):
            self.assertEqual(candidate.main(), 1 if changed or dirty or mismatch or omitted else 0)
        return json.loads((output / "VALIDATION.json").read_text())

    def test_candidate_requires_unchanged_commit_even_when_fingerprint_matches(self):
        self.assertFalse(self.run_candidate(changed=True)["passed"])

    def test_candidate_rejects_new_uncommitted_changes(self):
        self.assertFalse(self.run_candidate(dirty=True)["passed"])

    def test_candidate_rejects_a_report_for_different_source(self):
        self.assertFalse(self.run_candidate(mismatch=True)["passed"])

    def test_candidate_records_identity_and_report_hash_after_all_checks(self):
        report = self.run_candidate()
        self.assertTrue(report["passed"])
        self.assertEqual(report["scope"], "candidate")
        self.assertEqual(len(report["automated_sha256"]), 64)
        self.assertEqual(len(report["steps"]), 3)
        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["preparation"]["status"], "pass")
        self.assertEqual(report["finalization"]["status"], "pass")
        self.assertTrue(report["cleanup_confirmed"])

    def test_scoped_candidate_preserves_plan_and_pending_checks_and_rejects_omissions(self):
        report = self.run_candidate(scoped=True)
        self.assertTrue(report["passed"])
        self.assertFalse(report["release_ready"])
        self.assertNotIn("automated_sha256", report)
        self.assertEqual(len(report["integration_sha256"]), 64)
        self.assertEqual(report["pending"], ["完整自动验收", "目标环境验收"])
        self.assertTrue(report["not_run"])
        self.assertEqual(report["steps"][2]["arguments"][:4],
                         ["python3", "scripts/verify_integration.py", "--base", "f" * 40])
        self.assertFalse(self.run_candidate(scoped=True, omitted=True)["passed"])

    def test_candidate_retains_pending_live_evidence_without_claiming_release_ready(self):
        report = {
            "scope": "automated", "passed": True, "release_ready": False,
            "automated_passed": True, "tests": [{"status": "pass"}],
            "requirements": [{"status": "pass"}, {"status": "pending"}],
        }
        self.assertTrue(candidate.acceptance_outcome(report, 0))
        self.assertFalse(candidate.acceptance_outcome(report, 2))
        report["release_ready"] = True
        self.assertFalse(candidate.acceptance_outcome(report, 0))
        report.update(scope="release", passed=False, release_ready=False)
        self.assertFalse(candidate.acceptance_outcome(report, 2))

    def test_candidate_cannot_treat_skipped_tests_as_pending_live_evidence(self):
        report = {
            "scope": "automated", "passed": True, "release_ready": False,
            "automated_passed": True, "tests": [{"status": "skip"}],
            "requirements": [{"status": "pending"}],
        }
        self.assertFalse(candidate.acceptance_outcome(report, 0))


class ContextGateTests(unittest.TestCase):
    def classify_changes(self, path, *, symlink=False):
        with tempfile.TemporaryDirectory() as directory, chdir(directory):
            subprocess.run(["git", "init", "--quiet"], check=True)
            Path("runtime.rs").write_text("original runtime\n")
            validate_context.git("add", ".")
            commit = ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                      "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "fixture"]
            validate_context.git(*commit)
            base = validate_context.git("rev-parse", "HEAD")
            target = Path(path)
            target.parent.mkdir(parents=True, exist_ok=True)
            if symlink:
                target.symlink_to("../runtime.rs")
            else:
                target.write_text("changed\n")
            validate_context.git("add", ".")
            validate_context.git(*commit)
            return validate_context.context_only(base)

    def test_context_records_preserve_the_entire_implementation(self):
        self.assertTrue(self.classify_changes("context/journal/closeout.md"))

    def test_code_workflow_and_nonrecord_changes_require_full_candidate(self):
        for path in ("runtime.rs", ".github/workflows/gate.yml", "context/runtime.rs",
                     " context/hidden.md", "\ncontext/hidden.md", "context/hidden.md\n"):
            with self.subTest(path=path):
                self.assertFalse(self.classify_changes(path))

    def test_context_symlink_cannot_enter_the_records_path(self):
        self.assertFalse(self.classify_changes("context/linked.md", symlink=True))

    def check_base(self, *, observed_base=None, app=15368, conclusion="success", check_source=None):
        base = "a" * 40
        remote = {"object": {"sha": observed_base or base}}
        pages = [{"check_runs": [{
            "name": "Candidate gate", "head_sha": check_source or base,
            "app": {"id": app}, "status": "completed", "conclusion": conclusion,
        }]}]
        with mock.patch.object(subprocess, "check_output",
                               side_effect=[json.dumps(remote), json.dumps(pages)]):
            validate_context.require_base_gate("owner/repository", base)

    def test_records_require_a_successful_actions_gate_for_the_exact_base(self):
        self.check_base()
        for options in ({"app": 1}, {"conclusion": "skipped"},
                        {"conclusion": "failure"}, {"check_source": "b" * 40}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.check_base(**options)

    def test_records_reject_main_advancing_during_validation(self):
        with self.assertRaises(ValueError):
            self.check_base(observed_base="b" * 40)
