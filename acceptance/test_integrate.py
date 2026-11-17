import json
import fcntl
import os
from pathlib import Path
import tempfile
import time
import shlex
import subprocess
import sys
import unittest
from unittest.mock import patch

from scripts import integrate_candidate as integrate


class IntegrateTests(unittest.TestCase):
    def test_overlapping_cli_invocation_returns_busy_without_any_merge(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "candidate"
            with directory.with_name(".candidate.lock").open("a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with patch("sys.argv", ["integrate", "--resume", "--output", str(directory)]), \
                        patch.object(integrate, "start") as start:
                    self.assertEqual(integrate.main(), 75)
                start.assert_not_called()

    def test_separate_cli_processes_return_and_resume_without_a_foreground_monitor(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "started_at": integrate.now()}
            integrate.save(directory / "integration.json", state)
            driver = directory / "invoke.py"
            driver.write_text(
                "import json,sys\nfrom pathlib import Path\nfrom unittest.mock import patch\n"
                "from scripts import integrate_candidate as flow\n"
                "folder=Path(sys.argv[1]); phase=sys.argv[2]\n"
                "def effect(kind):\n"
                "    with (folder/'effects.jsonl').open('a') as stream:\n"
                "        stream.write(json.dumps({'kind':kind})+'\\n')\n"
                "def execute(arguments, **options):\n"
                "    if arguments[1]=='push': effect('push')\n"
                "def promote(*arguments):\n"
                "    effect('promote')\n"
                "    arguments[4].write_text(json.dumps({'release_status':'pending'}))\n"
                "run={'id':42,'run_attempt':1,'status':phase,"
                "'conclusion':'success' if phase=='completed' else None}\n"
                "sys.argv=['integrate','--resume','--output',str(folder)]\n"
                "with patch.object(flow,'require_checkout'), "
                "patch.object(flow,'git',return_value='base refs/heads/main'), "
                "patch.object(flow.subprocess,'run',side_effect=execute), "
                "patch.object(flow,'find_run',return_value=run), "
                "patch.object(flow,'promote',side_effect=promote):\n"
                "    sys.exit(flow.main())\n")
            environment = dict(os.environ, PYTHONPATH=str(integrate.ROOT))
            invocations = []
            for phase in ("in_progress", "completed", "completed"):
                started = time.monotonic()
                process = subprocess.run([sys.executable, str(driver), str(directory), phase],
                                         cwd=directory, env=environment, capture_output=True,
                                         text=True, timeout=30)
                self.assertEqual(process.returncode, 0, process.stderr)
                invocations.append({"process_returned": True, "returncode": process.returncode,
                                    "seconds": time.monotonic() - started,
                                    "output": json.loads(process.stdout)})
            self.assertEqual([entry["output"]["merge_status"] for entry in invocations],
                             ["waiting-ci", "merged", "merged"])
            effects = [json.loads(line) for line in (directory / "effects.jsonl").read_text().splitlines()]
            self.assertEqual(effects, [{"kind": "push"}, {"kind": "promote"}])
            self.assertEqual(invocations[1]["output"]["seconds"], invocations[2]["output"]["seconds"])
            evidence = os.environ.get("CSB_MERGE_TEST_EVIDENCE")
            if evidence:
                destination = Path(evidence).resolve()
                self.assertFalse(destination.is_relative_to(integrate.ROOT))
                destination.mkdir(parents=True, exist_ok=False)
                integrate.save(destination / "invocations.json", {
                    "scope": "isolated-state-machine", "external_operations": "mocked",
                    "invocations": invocations, "effects": effects,
                })
                (destination / "integration.json").write_bytes((directory / "integration.json").read_bytes())

    def test_pending_invocation_returns_then_resume_reuses_candidate_and_workflow(self):
        from argparse import Namespace
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "started_at": integrate.now()}
            integrate.save(directory / "integration.json", state)
            run = {"id": 42, "run_attempt": 1, "status": "in_progress", "conclusion": None}
            arguments = Namespace(output=directory, resume=True, timeout=0)
            def promoted(*values):
                values[4].write_text(json.dumps({"release_status": "pending"}))
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run") as execute, \
                    patch.object(integrate, "find_run", return_value=run), \
                    patch.object(integrate.time, "sleep") as sleep, \
                    patch.object(integrate, "promote", side_effect=promoted) as promote:
                integrate.start(arguments)
                pending = json.loads((directory / "integration.json").read_text())
                self.assertEqual(pending["merge_status"], "waiting-ci")
                self.assertEqual(pending["run"], 42)
                self.assertNotIn("last_error", pending)
                self.assertNotIn("errors", pending)
                self.assertEqual(pending["invocations"][0]["status"], "waiting-ci")
                promote.assert_not_called()
                sleep.assert_not_called()
                run.update(status="completed", conclusion="success")
                integrate.start(arguments)
                complete = json.loads((directory / "integration.json").read_text())
                self.assertEqual(complete["merge_status"], "merged")
                self.assertEqual(complete["release_status"], "pending")
                self.assertEqual(complete["started_at"], state["started_at"])
                self.assertEqual([entry["status"] for entry in complete["invocations"]],
                                 ["waiting-ci", "merged"])
                pushes = [call for call in execute.call_args_list if call.args[0][1] == "push"]
                self.assertEqual(len(pushes), 1)
                promote.assert_called_once()
                integrate.start(arguments)
                promote.assert_called_once()
                self.assertEqual(len([call for call in execute.call_args_list
                                      if call.args[0][1] == "push"]), 1)

    def test_pending_final_pr_checks_return_without_promoting_or_sleeping(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "pushed_at": integrate.now(), "pr_gates": {}}
            state["pr_gates"] = {"items": [7]}
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run"), \
                    patch.object(integrate, "find_run", return_value={
                        "id": 42, "run_attempt": 1, "status": "completed", "conclusion": "success"}), \
                    patch.object(integrate.pr_queue_gates, "verify", return_value=None), \
                    patch.object(integrate.time, "sleep") as sleep, \
                    patch.object(integrate, "promote") as promote:
                result = integrate.continue_candidate(state, Path(temporary), 0)
            self.assertEqual(result["waiting_on"], "pr")
            self.assertEqual(result["merge_status"], "waiting-ci")
            promote.assert_not_called()
            sleep.assert_not_called()

    def test_failed_query_keeps_workflow_identity_and_can_be_resumed(self):
        from argparse import Namespace
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            state = {"source": {"commit": "candidate"}, "base": "base", "branch": "branch",
                     "repository": "owner/repo", "started_at": integrate.now(),
                     "pushed_at": integrate.now(), "run": 42, "run_attempt": 1}
            integrate.save(directory / "integration.json", state)
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run"), \
                    patch.object(integrate, "find_run", side_effect=RuntimeError("query unavailable")), \
                    self.assertRaisesRegex(RuntimeError, "query unavailable"):
                integrate.start(Namespace(output=directory, resume=True, timeout=0))
            observed = json.loads((directory / "integration.json").read_text())
            self.assertEqual(observed["merge_status"], "infrastructure-blocked")
            self.assertEqual(observed["run"], 42)
            self.assertEqual(observed["started_at"], state["started_at"])
            self.assertEqual(observed["invocations"][0]["status"], "infrastructure-blocked")

    def test_unfrozen_version_fails_before_pushing_or_starting_ci(self):
        from argparse import Namespace
        from scripts import main_release_evidence
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "candidate"
            arguments = Namespace(output=directory, resume=False, base="base",
                                  repository="owner/repo", started_at=None)
            with patch.object(integrate, "git", side_effect=["codex/integration-fixture", "base"]), \
                    patch.object(integrate, "identity", return_value={"commit": "candidate"}), \
                    patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "context_only", return_value=False), \
                    patch.object(main_release_evidence, "require_version", side_effect=ValueError("fresh version")), \
                    patch.object(integrate.subprocess, "run") as run, \
                    self.assertRaisesRegex(ValueError, "fresh version"):
                integrate.start(arguments)
            run.assert_not_called()
            self.assertFalse(directory.exists())

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
                    patch.object(integrate, "github", return_value=json.dumps([{"jobs": [{
                        "name": "Candidate (linux-x86_64)", "conclusion": "failure",
                        "steps": [{"name": "TMPDIR=/tmp ./scripts/validate", "conclusion": "failure"}],
                    }]}])), \
                    patch.object(integrate, "promote") as promote, self.assertRaises(ValueError):
                integrate.start(Namespace(output=directory, resume=True, timeout=10))
            promote.assert_not_called()
            saved = json.loads((directory / "integration.json").read_text())
            self.assertEqual(saved["started_at"], state["started_at"])
            self.assertEqual(saved["run"], 1)
            self.assertEqual(saved["last_error"]["type"], "ValueError")
            self.assertEqual(saved["errors"][0], state["errors"][0])
            self.assertEqual(len(saved["errors"]), 2)
            self.assertEqual(saved["merge_status"], "needs-author")
            self.assertEqual(json.loads((directory / "failed-run-1-1.json").read_text())["run"], 1)
            self.assertEqual(saved["invocations"][-1]["workflow"]["run_attempt"], 1)

    def test_runner_failure_is_not_sent_to_author_and_workflow_identity_is_pinned(self):
        state = {"repository": "owner/repo"}
        run = {"id": 42, "run_attempt": 1, "conclusion": "failure"}
        with patch.object(integrate, "github", return_value=json.dumps([{"jobs": [{
                "name": "Candidate (macos-arm64)", "conclusion": "failure", "steps": [],
        }]}])), self.assertRaises(ValueError):
            integrate.failed_run(state, run)
        self.assertEqual(state["merge_status"], "infrastructure-blocked")
        pinned = {**run, "head_sha": "candidate", "head_branch": "branch", "event": "push",
                  "repository": {"full_name": "owner/repo"}, "path": ".github/workflows/plugin-candidate.yml"}
        with patch.object(integrate, "github", return_value=json.dumps(pinned)) as query:
            self.assertEqual(integrate.find_run("owner/repo", "branch", "candidate", 42)["id"], 42)
            self.assertIn("actions/runs/42", query.call_args.args[1])
            with self.assertRaises(ValueError):
                integrate.find_run("owner/repo", "branch", "changed", 42)

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
            def promoted(*arguments, **keywords):
                arguments[4].write_text(json.dumps({"merge_status": "merged",
                                                   "release_status": "pending"}))
            with patch.object(integrate, "require_checkout"), \
                    patch.object(integrate, "git", return_value="base refs/heads/main"), \
                    patch.object(integrate.subprocess, "run"), patch.object(integrate.time, "sleep"), \
                    patch.object(integrate, "find_run", side_effect=find), \
                    patch.object(integrate.pr_queue_gates, "verify", side_effect=check), \
                    patch.object(integrate, "promote", side_effect=promoted) as promote:
                integrate.continue_candidate(state, directory, 10)
            self.assertEqual(order, ["candidate", "pr", "pr"])
            self.assertEqual(promote.call_args.kwargs["pr_gates"], state["pr_gates"])
            self.assertEqual(state["pr_checks"], [{"head": "current"}])
            self.assertEqual(state["merge_status"], "merged")
            self.assertEqual(state["release_status"], "pending")

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
