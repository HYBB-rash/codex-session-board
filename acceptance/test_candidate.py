import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts import validate_candidate as candidate


class CandidateGateTests(unittest.TestCase):
    def run_candidate(self, *, changed=False, dirty=False, mismatch=False, failed=False):
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

        def execute(name, arguments, destination, **options):
            (destination / f"{name}.log").write_text("actual step transcript\n")
            (destination / f"{name}.stderr.log").write_text("step diagnostics\n")
            if "--report" in arguments:
                report = {
                    "automated_passed": True,
                    "build_sha256": "e" * 64 if mismatch else identity["fingerprint"],
                    "scope": "automated", "passed": True, "release_ready": True,
                    "tests": [{"status": "pass"}], "requirements": [{"status": "pass"}],
                }
                if failed:
                    report.update(automated_passed=False, passed=False, release_ready=False,
                                  tests=[{"status": "fail"}],
                                  requirements=[{"id": "R-01", "status": "pending"}])
                (destination / "automated.json").write_text(json.dumps(report))
            (destination / f"{name}.request.json").write_text("{}")
            (destination / f"{name}.lease.json").write_text(json.dumps({
                "run_id": options["run_id"], "name": name, "cleanup_confirmed": True}))
            return {"name": name, "arguments": arguments,
                    "returncode": 7 if failed and name == "step-3" else 0, "seconds": 1,
                    "log_sha256": hashlib.sha256((destination / f"{name}.log").read_bytes()).hexdigest(),
                    "cleanup_confirmed": True}

        arguments = ["validate", "--output", str(output)]
        with mock.patch.object(candidate, "ROOT", root), \
                mock.patch.object(candidate, "identity", side_effect=[identity, after]), \
                mock.patch.object(candidate, "git", return_value="dirty" if dirty else ""), \
                mock.patch.object(candidate.run.processes, "execute", side_effect=execute), \
                mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": str(root / "target")}), \
                mock.patch.object(candidate.sys, "argv", arguments):
            self.assertEqual(candidate.main(), 1 if changed or dirty or mismatch or failed else 0)
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
        failed = self.run_candidate(failed=True)
        self.assertEqual(failed["pending"], ["R-01"])
        self.assertEqual(len(failed["automated_sha256"]), 64)
        self.assertIn("step-3 退出 7", failed["first_failure"]["message"])
        self.assertFalse(failed["passed"])
        self.assertFalse(failed["release_ready"])

    def test_candidate_cannot_treat_skipped_tests_as_pending_live_evidence(self):
        report = {
            "scope": "automated", "passed": True, "release_ready": False,
            "automated_passed": True, "tests": [{"status": "skip"}],
            "requirements": [{"status": "pending"}],
        }
        self.assertFalse(candidate.acceptance_outcome(report, 0))
