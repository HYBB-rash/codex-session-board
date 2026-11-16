import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import BASE, HEAD, pull
from scripts import pr_queue_gates as gates


class PRQueueGateTests(unittest.TestCase):
    def setUp(self):
        self.gates = {"repository": "owner/repo", "items": [{
            "number": 7, "head": HEAD, "checks": ["changes"], "reviewers": ["owner"],
        }]}
        self.snapshot = {"head": HEAD, "state": "open", "ci": "passed", "items": []}

    def test_only_the_current_head_with_successful_checks_and_no_feedback_can_pass(self):
        for change, outcome in (({}, "passed"), ({"ci": "pending"}, "pending"),
                                ({"ci": "failed"}, "error"), ({"head": BASE}, "error"),
                                ({"state": "closed"}, "error"),
                                ({"items": [{"kind": "review"}]}, "error")):
            with self.subTest(change=change), \
                    mock.patch.object(gates.forge, "api", return_value=pull()), \
                    mock.patch.object(gates.forge, "snapshot", return_value={**self.snapshot, **change}), \
                    mock.patch.object(gates, "workflow", return_value={"run": 1}):
                if outcome == "error":
                    with self.assertRaises(ValueError):
                        gates.verify(self.gates)
                elif outcome == "pending":
                    self.assertIsNone(gates.verify(self.gates))
                else:
                    self.assertEqual(gates.verify(self.gates)[0]["head"], HEAD)

    def test_pr_changed_before_snapshot_does_not_reuse_old_ci(self):
        changed = pull()
        changed["head"]["sha"] = BASE
        with mock.patch.object(gates.forge, "api", return_value=changed), \
                mock.patch.object(gates.forge, "snapshot") as snapshot, self.assertRaises(ValueError):
            gates.verify(self.gates)
        snapshot.assert_not_called()

    def test_pr_changed_during_workflow_proof_cannot_pass(self):
        for change in ({"head": {**pull()["head"], "sha": BASE}}, {"state": "closed"}):
            with self.subTest(change=change), \
                    mock.patch.object(gates.forge, "api", side_effect=[pull(), {**pull(), **change}]), \
                    mock.patch.object(gates.forge, "snapshot", return_value=self.snapshot), \
                    mock.patch.object(gates, "workflow", return_value={"run": 1}), \
                    self.assertRaises(ValueError):
                gates.verify(self.gates)

    def test_already_merged_retry_still_requires_checks_on_the_exact_head(self):
        merged = {**pull(), "state": "closed", "merged": True}
        with mock.patch.object(gates.forge, "api", return_value=merged), \
                mock.patch.object(gates.forge, "snapshot",
                                  return_value={**self.snapshot, "state": "merged", "ci": "pending"}), \
                mock.patch.object(gates.forge, "check_state",
                                  return_value={"ci": "passed", "items": []}) as checks, \
                mock.patch.object(gates, "workflow", return_value={"run": 1}):
            self.assertEqual(gates.verify(self.gates)[0]["ci"], "passed")
        self.assertEqual(checks.call_args.args[:3], ("owner/repo", HEAD, ["changes"]))

    def test_gate_manifest_binds_the_real_queue_candidate_and_accepted_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            path = project / "queue.json"
            state = {"repository": "owner/repo", "project": str(project),
                     "frozen_candidate": {"candidate": HEAD}, "items": [
                         {"number": 7, "head": HEAD, "status": "accepted"},
                         {"number": 8, "head": BASE, "status": "needs-fix"},
                     ]}
            path.write_text(json.dumps(state))
            self.assertEqual(gates.from_queue(path, project, "owner/repo", HEAD), self.gates)
            for change in ({"repository": "other/repo"}, {"project": str(project / "other")},
                           {"rehearsal": True}, {"frozen_candidate": {"candidate": BASE}},
                           {"items": []}, {"items": state["items"][:1] * 2}):
                path.write_text(json.dumps({**copy.deepcopy(state), **change}))
                with self.subTest(change=change), self.assertRaises(ValueError):
                    gates.from_queue(path, project, "owner/repo", HEAD)

    def test_same_named_status_cannot_replace_the_real_pr_workflow(self):
        with mock.patch.object(gates.forge, "api", return_value=pull()), \
                mock.patch.object(gates.forge, "snapshot", return_value=self.snapshot), \
                mock.patch.object(gates.forge, "pages", return_value=[]):
            self.assertIsNone(gates.verify(self.gates))

    def test_workflow_proof_requires_the_exact_pr_head_workflow_and_successful_job(self):
        run = {"id": 1, "run_attempt": 2, "head_sha": HEAD, "event": "pull_request",
               "path": ".github/workflows/pr-checks.yml", "repository": {"full_name": "owner/repo"},
               "pull_requests": [{"number": 7, "head": {"sha": HEAD}}],
               "status": "completed", "conclusion": "success"}
        job = {"name": "changes", "head_sha": HEAD, "status": "completed", "conclusion": "success"}
        with mock.patch.object(gates.forge, "pages", side_effect=[[run], [job]]):
            self.assertEqual(gates.workflow("owner/repo", 7, HEAD)["attempt"], 2)
        for change in ({"head_sha": BASE}, {"event": "push"}, {"pull_requests": []},
                       {"path": ".github/workflows/other.yml"}, {"status": "in_progress"}):
            with self.subTest(change=change), \
                    mock.patch.object(gates.forge, "pages", return_value=[{**run, **change}]):
                self.assertIsNone(gates.workflow("owner/repo", 7, HEAD))
        for jobs in ([], [{**job, "head_sha": BASE}], [{**job, "conclusion": "skipped"}]):
            with self.subTest(jobs=jobs), \
                    mock.patch.object(gates.forge, "pages", side_effect=[[run], jobs]), \
                    self.assertRaises(ValueError):
                gates.workflow("owner/repo", 7, HEAD)
