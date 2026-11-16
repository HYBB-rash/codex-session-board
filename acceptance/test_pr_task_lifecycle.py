import copy
import json
import os
from pathlib import Path
import time
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import HEAD, feedback, saved, task
from scripts import pr_task_lifecycle as lifecycle
from scripts import pr_task as cli
from scripts.pr_task_state import load, locked, persist


class PRTaskLifecycleTests(unittest.TestCase):
    def prepare_event(self):
        state, snapshot = task(self)
        snapshot["items"] = [feedback()]
        event = lifecycle.create_event(state, snapshot, snapshot["items"])
        return state, snapshot, event

    def test_idle_snapshot_never_invokes_native_delivery_or_model(self):
        state, snapshot = task(self)
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.native, "delivery") as delivery, mock.patch.object(lifecycle.publish, "ready"):
            lifecycle.tick(state)
        delivery.assert_not_called()
        self.assertEqual(saved(state)["phase"], "waiting-ci")
        self.assertEqual(state["events"], [])

    def test_terminal_only_tick_does_not_start_a_repair_for_an_open_pr(self):
        state, snapshot = task(self)
        snapshot["items"] = [feedback()]
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), \
                mock.patch.object(lifecycle.native, "delivery") as delivery, \
                mock.patch.object(lifecycle, "create_event") as create:
            lifecycle.tick(state, force=True, terminal_only=True)
        delivery.assert_not_called()
        create.assert_not_called()
        self.assertEqual(state["events"], [])

    def test_read_failure_keeps_feedback_and_does_not_archive(self):
        state, _ = task(self)
        previous = copy.deepcopy(state)
        with mock.patch.object(lifecycle.forge, "snapshot", side_effect=RuntimeError("offline")), mock.patch.object(
                lifecycle.native, "archive") as archive:
            with self.assertRaises(RuntimeError):
                lifecycle.tick(state)
        archive.assert_not_called()
        self.assertEqual(saved(state), previous)

    def test_delivery_does_not_consume_feedback_and_duplicate_ticks_keep_one_event(self):
        state, snapshot = task(self)
        snapshot["items"] = [feedback()]
        def deliver(current, event):
            event["delivery"] = {"state": "sent", "turn_id": "turn", "attempted_at": "now"}
            return event["delivery"]
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle, "checkout", return_value=HEAD), mock.patch.object(
                lifecycle.native, "delivery", side_effect=deliver), mock.patch.object(lifecycle.publish, "labels"):
            lifecycle.tick(state)
            lifecycle.tick(state, force=True)
        self.assertEqual(len(state["events"]), 1)
        self.assertEqual(state["completed"], [])
        self.assertEqual(state["rounds"], 1)

    def test_busy_author_does_not_spend_a_repair_round(self):
        state, snapshot, event = self.prepare_event()
        with mock.patch.object(lifecycle.native, "delivery", return_value={"state": "waiting_idle"}):
            lifecycle.poll_event(state, event, snapshot)
        self.assertEqual(state["rounds"], 0)
        self.assertEqual(state["completed"], [])

    def test_head_change_before_send_supersedes_event_without_consuming_items(self):
        state, snapshot, event = self.prepare_event()
        snapshot["head"] = "c" * 40
        with mock.patch.object(lifecycle.native, "delivery") as delivery:
            lifecycle.poll_event(state, event, snapshot)
        delivery.assert_not_called()
        self.assertEqual(event["status"], "superseded")
        self.assertIsNone(state["active_event"])
        self.assertEqual(state["completed"], [])

    def test_deadline_does_not_restart_when_pending_event_is_polled(self):
        state, snapshot, event = self.prepare_event()
        state["deadline"] = time.time() - 1
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.native, "delivery") as delivery:
            lifecycle.tick(state)
        delivery.assert_not_called()
        self.assertEqual(state["phase"], "needs-human")
        self.assertEqual(event["status"], "pending")

    def test_terminal_state_is_checked_before_exhausted_budget(self):
        state, snapshot = task(self)
        state["deadline"] = time.time() - 1
        snapshot.update(state="merged", merge_commit="d" * 40)
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.native, "archive", return_value=True):
            lifecycle.tick(state)
        self.assertEqual(saved(state)["phase"], "complete")
        self.assertEqual(state["outcome"]["result"], "merged")

    def test_closed_unmerged_task_is_not_reported_as_merged(self):
        state, snapshot = task(self)
        snapshot["state"] = "closed"
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.native, "archive", return_value=False):
            lifecycle.tick(state)
        self.assertEqual(state["phase"], "finalizing")
        self.assertEqual(state["outcome"]["result"], "closed_without_merge")

    def test_claim_requires_original_session_and_unchanged_head(self):
        state, snapshot, event = self.prepare_event()
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": "another"}):
            with self.assertRaises(ValueError):
                lifecycle.claim(state, event["id"])
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle, "checkout", return_value=HEAD):
            self.assertEqual(lifecycle.claim(state, event["id"])["status"], "claimed")
            snapshot["head"] = "c" * 40
            with self.assertRaises(ValueError):
                lifecycle.claim(state, event["id"])

    def test_completed_turn_without_cli_completion_stays_unfinished(self):
        state, snapshot, event = self.prepare_event()
        with mock.patch.object(lifecycle.native, "delivery", return_value={
                "state": "sent", "turn_id": "turn", "turn_status": "completed"}):
            lifecycle.poll_event(state, event, snapshot)
        self.assertEqual(state["phase"], "needs-human")
        self.assertEqual(state["completed"], [])

    def test_successful_completion_requires_validation_and_exact_published_head(self):
        state, snapshot, event = self.prepare_event()
        event["status"] = "claimed"
        state["published"] = {"head": HEAD, "validation": state["directory"]}
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle, "checkout", return_value=HEAD), mock.patch.object(
                lifecycle.publish, "validation", side_effect=ValueError("stale report")):
            with self.assertRaises(ValueError):
                lifecycle.complete(state, event["id"], "fixed", "Done")
        self.assertEqual(state["completed"], [])
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle, "checkout", return_value=HEAD), mock.patch.object(
                lifecycle.publish, "validation"), mock.patch.object(
                lifecycle.publish, "comment_once", return_value="https://github.com/comment"):
            lifecycle.complete(state, event["id"], "fixed", "Done")
        self.assertEqual(state["completed"], ["feedback-key"])
        self.assertEqual(state["phase"], "waiting-ci")

    def test_failed_completion_comment_preserves_claim_for_safe_retry(self):
        state, snapshot, event = self.prepare_event()
        event["status"] = "claimed"
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.publish, "comment_once", side_effect=RuntimeError("lost response")):
            with self.assertRaises(RuntimeError):
                lifecycle.complete(state, event["id"], "blocked", "Need a decision")
        self.assertEqual(saved(state)["events"][0]["status"], "claimed")
        self.assertEqual(state["active_event"], event["id"])

    def test_blocked_disposition_does_not_trigger_another_automatic_round(self):
        state, snapshot, event = self.prepare_event()
        event["status"] = "claimed"
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.publish, "comment_once", return_value="https://github.com/comment"), mock.patch.object(
                lifecycle.native, "delivery") as delivery:
            lifecycle.complete(state, event["id"], "blocked", "Need a decision")
            lifecycle.tick(state, force=True)
        delivery.assert_not_called()
        self.assertEqual(state["phase"], "needs-human")

    def test_repair_limit_prevents_new_event_without_extending_deadline(self):
        state, snapshot = task(self)
        state["rounds"] = state["max_rounds"]
        snapshot["items"] = [feedback()]
        deadline = state["deadline"]
        with mock.patch.object(lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.native, "delivery") as delivery:
            lifecycle.tick(state)
        delivery.assert_not_called()
        self.assertEqual(state["deadline"], deadline)
        self.assertEqual(state["phase"], "needs-human")

    def test_second_process_cannot_claim_locked_task(self):
        state, _ = task(self)
        directory = Path(state["directory"])
        with locked(directory):
            with self.assertRaises(BlockingIOError):
                with locked(directory):
                    self.fail("A second operation acquired the active task")
        self.assertEqual(load(directory)["id"], state["id"])

    def test_same_failed_check_cannot_be_consumed_as_fixed(self):
        state, snapshot, event = self.prepare_event()
        event["status"] = "claimed"
        event["items"][0]["kind"] = "check"
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle.publish, "comment_once") as comment:
            with self.assertRaisesRegex(ValueError, "same CI failure"):
                lifecycle.complete(state, event["id"], "fixed", "Still red")
        comment.assert_not_called()
        self.assertEqual(state["completed"], [])
        self.assertEqual(event["status"], "claimed")

    def test_new_feedback_invalidates_same_head_readiness(self):
        state, snapshot = task(self)
        state["ready_head"] = HEAD
        lifecycle.create_event(state, snapshot, [feedback()])
        self.assertNotIn("ready_head", state)

    def test_explicit_retry_preserves_old_event_and_accepts_own_unfinished_work(self):
        state, snapshot, event = self.prepare_event()
        state["phase"] = "needs-human"
        arguments = cli.parser().parse_args(["resume", "--state", state["directory"], "--retry-failed"])
        with mock.patch.object(lifecycle.native, "delivery", return_value={"turn_status": "interrupted"}):
            cli.execute(state, arguments)
        self.assertEqual(event["status"], "superseded")
        retry = lifecycle.create_event(state, snapshot, [feedback()])
        self.assertEqual(retry["resume_from"], event["id"])
        with mock.patch.dict(os.environ, {"CODEX_THREAD_ID": state["route"]["thread_id"]}), mock.patch.object(
                lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                lifecycle, "checkout", return_value=HEAD) as checkout:
            lifecycle.claim(state, retry["id"])
        checkout.assert_called_once_with(state, clean=False)

    def test_unknown_or_running_delivery_cannot_be_explicitly_retried(self):
        state, _, event = self.prepare_event()
        arguments = cli.parser().parse_args(["resume", "--state", state["directory"], "--retry-failed"])
        for status in (None, "inProgress"):
            with mock.patch.object(lifecycle.native, "delivery", return_value={"turn_status": status}):
                with self.assertRaisesRegex(ValueError, "confirmed terminal"):
                    cli.execute(state, arguments)
        self.assertEqual(state["active_event"], event["id"])

    def test_completed_task_cannot_attach_or_resume(self):
        state, _ = task(self)
        state["phase"] = "complete"
        for command in (["attach", "--pr", "7"], ["resume"]):
            arguments = cli.parser().parse_args([*command, "--state", state["directory"]])
            with mock.patch.object(lifecycle.forge, "snapshot") as snapshot:
                with self.assertRaisesRegex(ValueError, "Ended"):
                    cli.execute(state, arguments)
            snapshot.assert_not_called()
