from argparse import Namespace
import copy
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import BASE, HEAD, pull
from scripts import pr_queue as queue
from scripts import pr_queue_cli as cli
from scripts import pr_queue_handoff as handoff
from scripts import pr_queue_registry as registry


CANDIDATE = "c" * 40


class PRQueueHandoffTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.state = {
            "repository": "owner/repo", "feedback_mode": "github", "project": str(self.directory),
            "checker": {"commit": "checker"}, "base": BASE, "prefix": CANDIDATE, "events": [],
            "started_at": queue.now(), "items": [{
                "number": 7, "head": HEAD, "base": BASE, "status": "accepted",
                "review": "review.json", "validation": "validation", "route": {"thread_id": "author"},
            }], "frozen_candidate": {"candidate": CANDIDATE, "status": "pending"},
        }
        self.pr = pull()
        self.main = BASE
        self.snapshot = {"head": HEAD, "state": "open", "ci": "passed", "items": []}
        self.arguments = Namespace(command="resume", item=None)
        for target, options in (
            (registry, {"registry": lambda: self.directory / "registry"}),
            (handoff.forge, {"api": self.api, "snapshot": lambda state: copy.deepcopy(self.snapshot)}),
            (queue, {"verify_checker": lambda state: None, "verify_entry": lambda state, item: None,
                     "clean": lambda project, source: None, "git": lambda *args: CANDIDATE}),
        ):
            for name, value in options.items():
                patch = mock.patch.object(target, name, side_effect=value)
                patch.start()
                self.addCleanup(patch.stop)
        queue.save(self.directory / "queue.json", self.state)

    def api(self, endpoint, *arguments, **options):
        if "/git/ref/" in endpoint:
            return {"object": {"sha": self.main}}
        if "/compare/" in endpoint:
            return {"status": "ahead"}
        return copy.deepcopy(self.pr)

    def run_round(self):
        state = json.loads((self.directory / "queue.json").read_text())
        cli.run(queue, state, self.directory, self.arguments)
        return json.loads((self.directory / "queue.json").read_text())

    def test_return_then_resume_reuses_candidate_and_duplicate_terminal_call_does_nothing(self):
        self.arguments.started_at = queue.now()
        def finish(state, directory):
            state["merge_status"] = "waiting-ci"
            state["integration"] = {"run": 42, "run_attempt": 1, "report": "integration.json"}
        with mock.patch.object(queue, "finish", side_effect=finish) as advance:
            first = self.run_round()
            self.assertEqual(first["handoff"]["result"], "waiting-ci")
            self.assertEqual(first["rounds"][0]["result"], "waiting-ci")
            self.assertEqual(registry.locate("owner/repo", 7), self.directory)
            self.assertEqual(first["integration"]["run"], 42)
            self.assertEqual(first["rounds"][0]["request_started_at"], self.arguments.started_at)
            self.assertGreaterEqual(first["rounds"][0]["request_seconds"], first["rounds"][0]["seconds"])
            def complete(state, directory):
                state.update(merge_status="merged", batch_merged=True, all_items_merged=True,
                             base=CANDIDATE, release_status="pending")
                state["items"][0]["status"] = "merged"
            advance.side_effect = complete
            second = self.run_round()
            self.assertEqual(second["handoff"]["result"], "merged")
            self.assertEqual(second["release_status"], "pending")
            self.assertEqual(second["started_at"], first["started_at"])
            self.assertEqual(second["rounds"][1]["between_calls"]["reason"], "waiting-ci")
            self.assertIn("existing-workflow", second["rounds"][1]["reused"])
            self.pr.update(merged=True, state="closed", merge_commit_sha=CANDIDATE)
            self.main = CANDIDATE
            with mock.patch.object(queue, "refresh_release") as publish, \
                    mock.patch.object(queue, "closeout", create=True) as cleanup:
                third = self.run_round()
            self.assertEqual(third["handoff"]["result"], "merged")
            self.assertEqual(advance.call_count, 2)
            publish.assert_not_called()
            cleanup.assert_not_called()
            self.arguments.command = "closeout"
            with mock.patch("scripts.pr_queue_closeout.closeout", side_effect=lambda state:
                            state.update(closeout={"complete": False})):
                pending_cleanup = self.run_round()
            self.assertEqual(pending_cleanup["handoff"]["result"], "merged")
            self.assertTrue(pending_cleanup["batch_merged"])
            self.assertFalse(pending_cleanup["closeout"]["complete"])

    def test_feedback_handoff_does_not_wait_notify_again_or_launch_author(self):
        self.snapshot["items"] = [{"kind": "feedback", "url": "https://example.invalid/feedback"}]
        with mock.patch.object(queue, "notify") as notify, mock.patch.object(queue, "finish") as finish:
            first = self.run_round()
            second = self.run_round()
        self.assertEqual(first["handoff"]["result"], "needs-author")
        self.assertEqual(second["handoff"]["next_actor"], "author")
        self.assertEqual(len(second["rounds"]), 2)
        self.assertIn("feedback", second["handoff"]["observation"]["items"][0]["snapshot"]["items"][0]["url"])
        notify.assert_not_called()
        finish.assert_not_called()

    def test_new_head_never_uses_old_review_or_candidate(self):
        self.pr["head"]["sha"] = "d" * 40
        with mock.patch.object(queue, "verify_entry") as review, mock.patch.object(queue, "finish") as finish:
            result = self.run_round()
        self.assertEqual(result["handoff"]["result"], "needs-evidence")
        self.assertEqual(result["items"][0]["head"], HEAD)
        self.assertEqual(result["frozen_candidate"]["candidate"], CANDIDATE)
        review.assert_not_called()
        finish.assert_not_called()

    def test_main_drift_preserves_old_candidate_and_requires_recombination(self):
        self.main = "e" * 40
        with mock.patch.object(queue, "finish") as finish:
            result = self.run_round()
        self.assertEqual(result["handoff"]["result"], "needs-user")
        self.assertIn("recombine", result["handoff"]["detail"])
        self.assertEqual(result["base"], BASE)
        self.assertEqual(result["handoff"]["next_actor"], "coordinator")
        finish.assert_not_called()

    def test_missing_evidence_and_query_failure_cannot_become_a_pass(self):
        with mock.patch.object(queue, "verify_entry", side_effect=FileNotFoundError("review.json")):
            first = self.run_round()
        self.assertEqual(first["handoff"]["result"], "needs-evidence")
        original = copy.deepcopy(first["observation"])
        with mock.patch.object(handoff.forge, "api", side_effect=RuntimeError("query unavailable")), \
                mock.patch.object(queue, "finish") as finish:
            second = self.run_round()
        self.assertEqual(second["handoff"]["result"], "infrastructure-blocked")
        self.assertEqual(second["observation"], original)
        self.assertEqual(second["last_error"]["detail"], "query unavailable")
        finish.assert_not_called()

    def test_closed_pr_is_terminal_without_merge_or_cleanup(self):
        self.pr["state"] = "closed"
        with mock.patch.object(queue, "finish") as finish, mock.patch.object(queue, "notify") as notify:
            result = self.run_round()
        self.assertEqual(result["handoff"]["result"], "closed-without-merge")
        self.assertEqual(result["items"][0]["status"], "closed")
        finish.assert_not_called()
        notify.assert_not_called()
        self.pr["state"] = "open"
        with mock.patch.object(queue, "finish") as finish:
            reopened = self.run_round()
        self.assertEqual(reopened["handoff"]["result"], "needs-user")
        finish.assert_not_called()

    def test_readonly_refresh_finds_registered_pr_and_never_changes_records(self):
        registry.register(self.directory, self.state)
        before = (self.directory / "queue.json").read_bytes()
        arguments = ["pr-queue", "status", "--repository", "owner/repo", "--pr", "7", "--refresh"]
        with mock.patch("sys.argv", arguments), redirect_stdout(io.StringIO()) as output, \
                mock.patch.object(queue, "refresh_release") as release:
            self.assertEqual(cli.main(queue), 0)
        self.assertEqual(json.loads(output.getvalue())["observation"]["main"], BASE)
        self.assertEqual((self.directory / "queue.json").read_bytes(), before)
        release.assert_not_called()
        self.snapshot["ci"] = "pending"
        with mock.patch("sys.argv", arguments), redirect_stdout(io.StringIO()) as output:
            self.assertEqual(cli.main(queue), 0)
        self.assertEqual(json.loads(output.getvalue())["handoff"]["result"], "waiting-ci")
        self.assertEqual((self.directory / "queue.json").read_bytes(), before)

    def test_ambiguous_or_missing_queue_records_are_not_silently_replaced(self):
        registry.register(self.directory, self.state)
        second = self.directory / "second"
        second.mkdir()
        queue.save(second / "queue.json", self.state)
        registry.register(second, self.state)
        with self.assertRaisesRegex(ValueError, "Multiple"):
            registry.locate("owner/repo", 7)
        (second / "queue.json").unlink()
        with self.assertRaises(FileNotFoundError):
            registry.locate("owner/repo", 7)

    def test_interrupted_round_is_retained_and_next_round_can_return(self):
        self.state["rounds"] = [{"command": "resume", "started_at": queue.now(), "result": "running"}]
        queue.save(self.directory / "queue.json", self.state)
        with mock.patch.object(queue, "finish", side_effect=RuntimeError("interrupted query")):
            result = self.run_round()
        self.assertEqual(result["rounds"][0]["result"], "interrupted")
        self.assertNotIn("finished_at", result["rounds"][0])
        self.assertEqual(result["rounds"][1]["result"], "infrastructure-blocked")

    def test_first_ready_time_survives_repairs_and_includes_between_call_wait(self):
        self.pr.update(created_at="2026-11-17T00:00:00Z")
        events = [
            {"event": "ready_for_review", "created_at": "2026-11-17T01:00:00Z"},
            {"event": "convert_to_draft", "created_at": "2026-11-17T02:00:00Z"},
            {"event": "ready_for_review", "created_at": "2026-11-17T03:00:00Z"},
        ]
        with mock.patch.object(handoff.forge, "pages", return_value=events), \
                mock.patch.object(queue, "finish", side_effect=lambda state, directory:
                                  state.update(merge_status="waiting-ci")):
            first = self.run_round()
        self.assertEqual(first["items"][0]["first_ready_at"], "2026-11-17T01:00:00Z")
        self.pr.update(merged=True, state="closed", merge_commit_sha=CANDIDATE,
                       merged_at="2026-11-17T04:00:00Z")
        self.main = CANDIDATE
        with mock.patch.object(handoff.forge, "pages") as timeline:
            final = self.run_round()
        self.assertEqual(final["handoff"]["items"][0]["ready_to_main_seconds"], 10800)
        timeline.assert_not_called()

    def test_batch_can_finish_accepted_items_without_waiting_for_rejected_item(self):
        rejected = {**self.state["items"][0], "number": 8, "status": "needs-fix",
                    "feedback": {"kind": "merge", "comment": "https://example.invalid/feedback"}}
        self.state["items"].append(rejected)
        queue.save(self.directory / "queue.json", self.state)
        observation = {"main": BASE, "result": "needs-author", "detail": "",
                       "items": [{"number": 7, "result": "ready"}, {"number": 8, "result": "needs-author"}]}
        def finish(state, directory):
            state.update(merge_status="merged", batch_merged=True, all_items_merged=False)
            state["items"][0]["status"] = "merged"
        with mock.patch.object(handoff, "observe", return_value=observation), \
                mock.patch.object(queue, "finish", side_effect=finish) as advance:
            result = self.run_round()
        self.assertTrue(result["batch_merged"])
        self.assertEqual(result["handoff"]["result"], "needs-author")
        self.assertEqual([item["status"] for item in result["items"]], ["merged", "needs-fix"])
        advance.assert_called_once()
