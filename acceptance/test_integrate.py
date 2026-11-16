import json
import os
from pathlib import Path
import tempfile
import shlex
import subprocess
import sys
import unittest
from unittest.mock import patch

from scripts import integrate_candidate as integrate


class IntegrateTests(unittest.TestCase):
    def test_launcher_uses_the_existing_delivery_environment_before_default_python(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            delivery = directory / "codex-session-board/delivery-venv/bin/python"
            delivery.parent.mkdir(parents=True)
            delivery.write_text("#!/bin/sh\nexec " + shlex.quote(sys.executable) + ' "$@"\n')
            delivery.chmod(0o700)
            fallback = directory / "bin/python3"
            fallback.parent.mkdir()
            fallback.write_text("#!/bin/sh\nexit 97\n")
            fallback.chmod(0o700)
            environment = dict(os.environ, XDG_DATA_HOME=temporary,
                               PATH=str(fallback.parent) + os.pathsep + os.environ["PATH"])
            result = subprocess.run([str(integrate.ROOT / "scripts/integrate"), "--help"],
                                    env=environment, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("--pr-gates", result.stdout)

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
                "errors": [{"type": "PreviousFailure", "detail": "retained"}],
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
            self.assertEqual(saved["errors"][0], state["errors"][0])
            self.assertEqual(len(saved["errors"]), 2)

    def test_candidate_build_finishes_before_waiting_for_pr_ci_and_checks_reach_promoter(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "started_at": "2026-11-16T00:00:00+00:00",
                     "pushed_at": "2026-11-16T00:01:00+00:00", "pr_gates": {"items": [7]}}
            run = {"id": 1, "run_attempt": 1, "status": "completed", "conclusion": "success"}
            order = []
            def find(*arguments):
                order.append("candidate")
                return run
            def check(*arguments):
                order.append("pr")
                return None if order.count("pr") == 1 else [{"head": "current"}]
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run"), patch.object(integrate.time, "sleep"), \
                    patch.object(integrate, "find_run", side_effect=find), \
                    patch.object(integrate.pr_queue_gates, "verify", side_effect=check), \
                    patch.object(integrate, "promote") as promote:
                integrate.continue_candidate(state, directory, 10)
            self.assertEqual(order, ["candidate", "pr", "pr"])
            self.assertEqual(promote.call_args.kwargs["pr_gates"], state["pr_gates"])
            self.assertEqual(state["pr_checks"], [{"head": "current"}])

    def test_resume_adopts_missing_pr_gates_and_rejects_different_frozen_inputs(self):
        from argparse import Namespace
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "started_at": "2026-11-16T00:00:00+00:00"}
            incoming = {"items": [{"number": 7, "head": "current"}]}
            arguments = Namespace(output=directory, resume=True, timeout=10, pr_gates=Path("queue.json"))
            for previous, blocked in ((None, False), (incoming, False), ({"items": []}, True)):
                saved = {**state, **({"pr_gates": previous} if previous is not None else {})}
                integrate.save(directory / "integration.json", saved)
                with patch.object(integrate.pr_queue_gates, "from_queue", return_value=incoming), \
                        patch.object(integrate, "continue_candidate", return_value={
                            **state, "run": 1, "seconds": 1}) as continuation:
                    if blocked:
                        with self.assertRaises(ValueError):
                            integrate.start(arguments)
                        continuation.assert_not_called()
                    else:
                        integrate.start(arguments)
                        self.assertEqual(continuation.call_args.args[0]["pr_gates"], incoming)
