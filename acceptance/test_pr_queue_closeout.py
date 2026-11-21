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
