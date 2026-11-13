import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts import validate_candidate as candidate


class CandidateGateTests(unittest.TestCase):
    def run_candidate(self, *, changed=False, dirty=False, mismatch=False):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        parent = Path(directory.name)
        root = parent / "source"
        root.mkdir()
        output = parent / "result"
        identity = {"commit": "a" * 40, "tree": "b" * 40, "fingerprint": "c" * 64}
        after = {**identity, **({"commit": "d" * 40} if changed else {})}

        def execute(name, arguments, destination):
            (destination / f"{name}.log").write_text("actual step transcript\n")
            if "--report" in arguments:
                report = {"automated_passed": True, "build_sha256": "e" * 64 if mismatch else identity["fingerprint"]}
                (destination / "automated.json").write_text(json.dumps(report))
            return {"name": name, "arguments": arguments, "returncode": 0, "seconds": 1}

        with mock.patch.object(candidate, "ROOT", root), \
                mock.patch.object(candidate, "identity", side_effect=[identity, after]), \
                mock.patch.object(candidate, "git", return_value="dirty" if dirty else ""), \
                mock.patch.object(candidate, "execute", side_effect=execute), \
                mock.patch.object(candidate.sys, "argv", ["validate", "--output", str(output)]):
            if changed or dirty or mismatch:
                with self.assertRaises(ValueError):
                    candidate.main()
            else:
                self.assertEqual(candidate.main(), 0)
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
