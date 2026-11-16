import copy
import subprocess
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import HEAD, task
from scripts import pr_queue_closeout as closeout


class PRQueueCloseoutTests(unittest.TestCase):
    def fixture(self):
        state, _ = task(self)
        state.update(checker={"commit": "checker"}, published={"head": HEAD}, cleanup_pending=False)
        item = {"number": state["pr"], "head": HEAD, "status": "merged", "route": state["route"],
                "author_task_state": state["directory"]}
        queue = {"repository": state["repository"], "checker": state["checker"],
                 "batch_merged": True, "items": [item]}
        return state, item, queue

    def test_cleanup_reuses_task_tick_and_completed_tasks_do_not_run_again(self):
        state, item, queue = self.fixture()
        complete = {**state, "phase": "complete", "outcome": {"result": "merged"}}
        with mock.patch.object(closeout, "load", side_effect=[state, complete]), \
                mock.patch.object(closeout.subprocess, "run",
                                  return_value=subprocess.CompletedProcess([], 0)) as run:
            result = closeout.closeout(queue)
        self.assertTrue(result["complete"])
        self.assertIn("tick", run.call_args.args[0])
        self.assertIn("--force", run.call_args.args[0])
        self.assertIn("--terminal-only", run.call_args.args[0])
        with mock.patch.object(closeout, "load", return_value=complete), \
                mock.patch.object(closeout.subprocess, "run") as run:
            self.assertEqual(closeout.author(queue, item)["status"], "complete")
        run.assert_not_called()

    def test_other_task_source_checker_or_route_cannot_be_archived(self):
        state, item, queue = self.fixture()
        changes = ({"pr": 8}, {"repository": "other/repo"}, {"checker": {}},
                   {"published": {"head": "b" * 40}},
                   {"route": {**state["route"], "thread_id": "other"}})
        for change in changes:
            with self.subTest(change=change), mock.patch.object(
                    closeout, "load", return_value={**copy.deepcopy(state), **change}), \
                    mock.patch.object(closeout.subprocess, "run") as run:
                result = closeout.closeout(queue)
                self.assertFalse(result["complete"])
                self.assertEqual(result["authors"][0]["status"], "pending")
            run.assert_not_called()

    def test_unregistered_pending_and_retried_cleanup_preserve_delivery_and_start_time(self):
        state, item, queue = self.fixture()
        item.pop("author_task_state")
        result = closeout.closeout(queue)
        self.assertEqual(result["authors"][0]["status"], "not_registered")
        started = result["started_at"]
        queue["items"].append({**item, "number": 8, "status": "needs-fix"})
        with mock.patch.object(closeout, "author", return_value={"number": state["pr"], "status": "complete"}):
            result = closeout.closeout(queue)
        self.assertTrue(result["complete"])
        self.assertEqual(len(result["authors"]), 1)
        self.assertEqual(result["started_at"], started)
        self.assertTrue(queue["batch_merged"])
        queue["batch_merged"] = False
        with self.assertRaises(ValueError):
            closeout.closeout(queue)
