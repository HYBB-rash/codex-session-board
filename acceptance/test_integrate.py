import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts import integrate_candidate as integrate


class IntegrateTests(unittest.TestCase):
    def test_run_selection_uses_exact_source_branch_and_latest_attempt(self):
        runs = [
            {"id": 1, "head_sha": "candidate", "head_branch": "branch", "event": "push"},
            {"id": 2, "head_sha": "other", "head_branch": "branch", "event": "push"},
            {"id": 3, "head_sha": "candidate", "head_branch": "other", "event": "push"},
            {"id": 4, "head_sha": "candidate", "head_branch": "branch", "event": "workflow_dispatch"},
        ]
        with patch.object(integrate, "github", return_value=json.dumps({"workflow_runs": runs})):
            self.assertEqual(integrate.find_run("owner/repo", "branch", "candidate")["id"], 4)
            self.assertIsNone(integrate.find_run("owner/repo", "branch", "missing"))

    def test_wrong_checkout_dirty_source_or_changed_branch_blocks_resume(self):
        state = {"source": {"commit": "candidate"}, "branch": "codex/integration-fixture"}
        with patch.object(integrate, "identity", return_value=state["source"]), \
                patch.object(integrate, "git", side_effect=["", state["branch"]]):
            integrate.require_checkout(state)
        for source, status, branch in (
            ({}, "", state["branch"]),
            (state["source"], " M index.html", state["branch"]),
            (state["source"], "", "main"),
        ):
            with patch.object(integrate, "identity", return_value=source), \
                    patch.object(integrate, "git", side_effect=[status, branch]), \
                    self.assertRaises(ValueError):
                integrate.require_checkout(state)

    def test_failed_run_is_recorded_without_promoting_or_restarting_the_clock(self):
        from argparse import Namespace
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {
                "source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                "repository": "owner/repo", "started_at": "2026-11-14T00:00:00+00:00",
                "pushed_at": "2026-11-14T00:01:00+00:00",
            }
            integrate.save(directory / "integration.json", state)
            run = {"id": 1, "run_attempt": 1, "status": "completed", "conclusion": "failure"}
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run"), \
                    patch.object(integrate, "find_run", return_value=run), \
                    patch.object(integrate, "promote") as promote, self.assertRaises(ValueError):
                integrate.start(Namespace(output=directory, resume=True, timeout=10))
            promote.assert_not_called()
            saved = json.loads((directory / "integration.json").read_text())
            self.assertEqual(saved["started_at"], state["started_at"])
            self.assertEqual(saved["run"], 1)
            self.assertEqual(saved["last_error"]["type"], "ValueError")
