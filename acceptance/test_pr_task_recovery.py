import copy
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess
import time
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import HEAD, saved, task
from scripts import pr_task_init as setup
from scripts import pr_task_native as native
from scripts import pr_task_publish as publish
from scripts import pr_task_scheduler as scheduler
from scripts.pr_task_state import digest, persist, save


class PRTaskRecoveryTests(unittest.TestCase):
    def rebind_case(self):
        state, _ = task(self)
        event = {"id": "event", "prompt": "repair", "status": "pending"}
        state["events"].append(event)
        state["active_event"] = event["id"]
        receipt = {"state": "pending", "request_id": event["id"],
                   "thread_id": state["route"]["thread_id"], "body_sha256": digest(event["prompt"]),
                   "route_sha256": digest(state["route"])}
        save(native.receipt_path(state, event), receipt)
        persist(state)
        new_route = {**state["route"], "socket": "/new.sock"}
        register = mock.patch.object(native, "register", return_value=(new_route, state["session_anchor"]))
        register.start()
        self.addCleanup(register.stop)
        connection = mock.patch.object(native.transport, "connect_endpoint")
        connection.start()
        self.addCleanup(connection.stop)
        latest = mock.patch.object(native, "latest", return_value=({}, {"id": "initial"}, []))
        latest.start()
        self.addCleanup(latest.stop)
        return state, event, new_route

    def test_rebind_atomically_switches_route_and_receipt_pointer(self):
        state, event, route = self.rebind_case()
        old_path = native.receipt_path(state, event)
        old_receipt = old_path.read_bytes()
        setup.rebind(state, Path("/new.sock"))
        self.assertEqual(saved(state)["route"], route)
        self.assertNotEqual(native.receipt_path(state, event), old_path)
        self.assertEqual(old_path.read_bytes(), old_receipt)
        self.assertEqual(json.loads(native.receipt_path(state, event).read_text())["route_sha256"], digest(route))

    def test_crash_before_rebind_commit_keeps_old_route_and_receipt_consistent(self):
        state, event, _ = self.rebind_case()
        before = saved(state)
        with mock.patch.object(setup, "persist", side_effect=OSError("power loss")):
            with self.assertRaises(OSError):
                setup.rebind(state, Path("/new.sock"))
        durable = saved(state)
        self.assertEqual(durable, before)
        receipt = json.loads(native.receipt_path(durable, durable["events"][0]).read_text())
        self.assertEqual(receipt["route_sha256"], digest(durable["route"]))

    def test_unknown_delivery_cannot_rebind_without_native_proof(self):
        state, event, _ = self.rebind_case()
        path = native.receipt_path(state, event)
        receipt = json.loads(path.read_text())
        receipt["state"] = "delivery_unknown"
        save(path, receipt)
        before = saved(state)
        with mock.patch.object(native.transport, "observe"):
            with self.assertRaisesRegex(ValueError, "Unknown delivery"):
                setup.rebind(state, Path("/new.sock"))
        self.assertEqual(saved(state), before)

    def test_expired_rpc_budget_sends_no_mutation(self):
        connection = mock.Mock()
        rpc = native.transport.Rpc(connection)
        rpc.deadline = time.monotonic() - 1
        with self.assertRaisesRegex(TimeoutError, "before thread/archive"):
            rpc.request("thread/archive", {"threadId": "author"})
        connection.send.assert_not_called()

    def test_uncertain_publication_requires_unchanged_report_and_ancestor(self):
        state, _ = task(self)
        directory = Path(state["directory"])
        report = directory / "VALIDATION.json"
        save(report, {"passed": True, "project": state["project"], "source": {"commit": HEAD}})
        state["publish_intent"] = {"head": HEAD, "validation": str(directory),
                                   "validation_sha256": hashlib.sha256(report.read_bytes()).hexdigest()}
        with mock.patch.object(publish.subprocess, "run") as ancestor:
            self.assertTrue(publish.own_publish_intent(state, HEAD, "c" * 40))
        self.assertIn("--is-ancestor", ancestor.call_args.args[0])
        self.assertFalse(publish.own_publish_intent(state, "d" * 40, "c" * 40))
        report.write_text("{}")
        with self.assertRaisesRegex(ValueError, "report changed"):
            publish.own_publish_intent(state, HEAD, "c" * 40)

    def test_lost_comment_response_reuses_exact_marker(self):
        marker = "<!-- pr-task:result id -->"
        entry = {"body": f"{marker}\nDone", "html_url": "https://github.com/comment"}
        with mock.patch.object(publish.forge, "pages", return_value=[entry]), mock.patch.object(
                publish.forge, "api") as api:
            self.assertEqual(publish.comment_once("owner/repo", 7, marker, "Done"), entry["html_url"])
            with self.assertRaisesRegex(ValueError, "different content"):
                publish.comment_once("owner/repo", 7, marker, "Changed")
        api.assert_not_called()

    def test_scheduler_only_accepts_service_not_found_as_absent(self):
        state, _ = task(self)
        state["scheduler"] = {"label": "owned"}
        for code in (112, 1):
            with mock.patch.object(scheduler, "command", return_value=mock.Mock(returncode=code, stderr="error")):
                with self.assertRaisesRegex(RuntimeError, "cannot confirm"):
                    scheduler.present(state)
        with mock.patch.object(scheduler, "command", return_value=mock.Mock(returncode=113)):
            self.assertFalse(scheduler.present(state))

    def test_scheduler_keeps_virtualenv_interpreter_and_checks_background_write_access(self):
        state, _ = task(self)
        directory = Path(state["directory"])
        with mock.patch.object(scheduler.sys, "platform", "darwin"), mock.patch.object(
                scheduler.sys, "executable", str(directory / "venv/bin/python")), mock.patch.object(
                scheduler.Path, "home", return_value=directory), mock.patch.dict(
                os.environ, {"CODEX_GITHUB_COMMAND": '["gh"]'}), mock.patch.object(
                scheduler.shutil, "which", return_value="/fixture/gh"), mock.patch.object(
                scheduler.subprocess, "run", return_value=mock.Mock(stdout="owner/repo\n")) as run, mock.patch.object(
                scheduler, "present", side_effect=[False, True]), mock.patch.object(scheduler, "command"):
            scheduler.install(state)
        definition = plistlib.loads(Path(state["scheduler"]["plist"]).read_bytes())
        self.assertEqual(definition["ProgramArguments"][0], str(directory / "venv/bin/python"))
        self.assertIn("probe", run.call_args_list[1].args[0])
        self.assertTrue(definition["AbandonProcessGroup"])

    def test_cleanup_preserves_foreign_plist_and_retries_after_bootout(self):
        state, _ = task(self)
        path = Path(state["directory"]) / "owned.plist"
        definition = {"Label": "owned"}
        path.write_bytes(plistlib.dumps({"Label": "foreign"}))
        state["scheduler"] = {"label": "owned", "plist": str(path), "sha256": digest(definition)}
        with mock.patch.object(scheduler, "command") as command:
            with self.assertRaisesRegex(ValueError, "ownership changed"):
                scheduler.remove(state)
        command.assert_not_called()
        self.assertTrue(path.exists())
        path.write_bytes(plistlib.dumps(definition))
        with mock.patch.object(scheduler, "present", side_effect=[True, True]), mock.patch.object(
                scheduler, "command", return_value=mock.Mock(returncode=5, stderr="busy")):
            with self.assertRaisesRegex(RuntimeError, "unconfirmed"):
                scheduler.remove(state)
        self.assertFalse(path.exists())
        with mock.patch.object(scheduler, "present", return_value=False):
            scheduler.remove(state)
        self.assertEqual(saved(state)["scheduler"]["status"], "removed")

    def test_finalizer_uses_new_terminal_outcome_and_handles_no_scheduler(self):
        state, snapshot = task(self)
        state.update(phase="finalizing", outcome={"result": "closed_without_merge", "head": "old"})
        snapshot.update(state="merged", merge_commit="d" * 40)
        with mock.patch.object(scheduler.forge, "snapshot", return_value=snapshot), mock.patch.object(
                scheduler.native, "archive", return_value=True):
            scheduler.finalize(state)
        self.assertEqual(saved(state)["phase"], "complete")
        self.assertEqual(state["outcome"]["result"], "merged")
        self.assertEqual(state["outcome"]["head"], HEAD)
        self.assertEqual(state["outcome"]["merge_commit"], "d" * 40)
