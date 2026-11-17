import copy
import json
from pathlib import Path
import subprocess
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import HEAD, feedback, pull, saved, task
from acceptance.pr_task_runtime_fixture import command, runtime
from scripts import pr_task as cli
from scripts import pr_task_init as setup
from scripts import pr_task_manual as manual
from scripts.pr_task_state import persist


class PRTaskManualTests(unittest.TestCase):
    def test_registration_needs_no_client_and_mode_cannot_change(self):
        old, database, _ = runtime(self)
        project = Path(old["project"])
        head = json.loads(database.read_text())["pr"]["head"]["sha"]
        subprocess.run(["git", "update-ref", "refs/remotes/origin/main", head],
                       cwd=project, check=True, capture_output=True)
        arguments = cli.parser().parse_args([
            "init", "--mode", "manual", "--state", str(Path(old["directory"]) / "registered"),
            "--project", str(project), "--repository", "owner/repo",
        ])
        arguments.thread = None
        def api(endpoint):
            return {"login": "owner"} if endpoint == "user" else {"full_name": "owner/repo"}
        with mock.patch.object(setup.forge, "api", side_effect=api), mock.patch.object(
                setup.native, "register", side_effect=AssertionError("Client must not be contacted")), mock.patch.object(
                setup, "checker_identity", return_value={"commit": "frozen"}):
            first = setup.initialize(arguments)
            self.assertEqual(setup.initialize(arguments), first)
            self.assertEqual(first["mode"], "manual")
            self.assertNotIn("socket", first["route"])
            self.assertIsNone(first["scheduler"])
            arguments.mode, arguments.thread, arguments.socket = "automatic", first["route"]["thread_id"], Path("/rpc")
            with self.assertRaisesRegex(ValueError, "mode cannot change"):
                setup.initialize(arguments)

    def test_refresh_reads_new_feedback_without_writing_task_or_pr(self):
        state, database, environment = runtime(self)
        state["mode"] = "manual"
        persist(state)
        before = saved(state)
        data = json.loads(database.read_text())
        data["checks"][0]["conclusion"] = "failure"
        database.write_text(json.dumps(data))
        result = command(state, environment, "status", "--refresh")
        self.assertEqual(result["handoff"]["next_action"], "address-feedback")
        self.assertEqual(result["handoff"]["feedback"][0]["kind"], "check")
        self.assertEqual(saved(state), before)
        data = json.loads(database.read_text())
        self.assertTrue(all(call["method"] == "GET" for call in data["calls"]))
        self.assertEqual(data["comments"], [])
        self.assertEqual(data["labels"], [])

    def test_manual_tasks_reject_automatic_commands_without_pausing(self):
        state, _, environment = runtime(self)
        state["mode"] = "manual"
        persist(state)
        before = saved(state)
        for action in ("tick", "schedule", "probe", "pause", "resume"):
            with self.subTest(action=action):
                result = command(state, environment, action, check=False)
                self.assertEqual(result.returncode, 1)
                self.assertIn("Manual tasks", result.stderr)
                self.assertEqual(saved(state), before)

    def test_submit_reuses_publisher_without_installing_scheduler(self):
        state, _ = task(self)
        state["mode"] = "manual"
        arguments = cli.parser().parse_args(["submit", "--state", state["directory"]])
        with mock.patch.object(cli.publish, "submit") as submit, mock.patch.object(
                cli.scheduler, "install", side_effect=AssertionError("No scheduler")):
            cli.execute(state, arguments)
        submit.assert_called_once_with(state, None, None)

    def test_ready_requires_final_validated_head_passed_ci_and_no_feedback(self):
        state, snapshot = task(self)
        state.update(mode="manual", published={"head": HEAD})
        snapshot["ci"] = "passed"
        cases = (
            {**snapshot, "ci": "pending"}, {**snapshot, "ci": "failed"},
            {**snapshot, "items": [feedback()]}, {**snapshot, "head": "c" * 40},
            {**snapshot, "state": "closed"},
        )
        with mock.patch.object(manual, "checkout", return_value=HEAD), mock.patch.object(
                manual.publish, "ready") as ready:
            for changed in cases:
                with self.subTest(snapshot=changed), mock.patch.object(
                        manual.forge, "snapshot", return_value=changed):
                    with self.assertRaises(ValueError):
                        manual.ready(state)
            ready.assert_not_called()
            with mock.patch.object(manual.forge, "snapshot", return_value=snapshot):
                manual.ready(state)
            ready.assert_called_once_with(state, snapshot, refresh=True)
        self.assertEqual(saved(state)["phase"], "awaiting-review")

    def test_finalize_distinguishes_closed_from_merged_and_keeps_work(self):
        state, snapshot = task(self)
        state["mode"] = "manual"
        for status, expected in (("closed", "closed_without_merge"), ("merged", "merged")):
            snapshot.update(state=status, merge_commit="d" * 40 if status == "merged" else None)
            with mock.patch.object(manual.forge, "snapshot", return_value=snapshot):
                manual.finalize(state)
                before = copy.deepcopy(saved(state))
                manual.finalize(state)
            self.assertEqual(saved(state), before)
            self.assertEqual(state["outcome"]["result"], expected)
            self.assertEqual(state["archive_receipt"]["status"], "not_requested")
            self.assertTrue(Path(state["directory"]).exists())
        snapshot["state"] = "open"
        with mock.patch.object(manual.forge, "snapshot", return_value=snapshot):
            with self.assertRaisesRegex(ValueError, "reopened"):
                manual.finalize(state)

    def test_refresh_terminal_or_failed_request_never_finalizes(self):
        state, snapshot = task(self)
        state.update(mode="manual", published={"head": HEAD})
        persist(state)
        before = saved(state)
        arguments = cli.parser().parse_args(["status", "--refresh", "--state", state["directory"]])
        snapshot["state"] = "merged"
        with mock.patch.object(manual.forge, "snapshot", return_value=snapshot):
            self.assertEqual(cli.execute(state, arguments)["handoff"]["next_action"], "finalize")
        with mock.patch.object(manual.forge, "snapshot", side_effect=manual.forge.ForgeError("offline")):
            with self.assertRaisesRegex(manual.forge.ForgeError, "offline"):
                cli.execute(state, arguments)
        self.assertEqual(saved(state), before)

    def test_attach_and_refresh_do_not_reuse_readiness_for_changed_head_or_ci(self):
        state, snapshot = task(self)
        state.update(mode="manual", published={"head": HEAD}, ready_head=HEAD)
        snapshot["ci"] = "passed"
        arguments = cli.parser().parse_args(["attach", "--state", state["directory"], "--pr", "7"])
        with mock.patch.object(cli.forge, "snapshot", return_value=snapshot), mock.patch.object(
                cli, "checkout", return_value=HEAD), mock.patch.object(
                cli.scheduler, "install", side_effect=AssertionError("No scheduler")):
            cli.execute(state, arguments)
            self.assertEqual(manual.observe(state)["handoff"]["next_action"], "independent-review")
            snapshot["ci"] = "pending"
            self.assertEqual(manual.observe(state)["handoff"]["next_action"], "wait-ci")
            snapshot.update(head="c" * 40, ci="passed")
            self.assertEqual(manual.observe(state)["handoff"]["next_action"], "submit")

    def test_explicit_ready_revalidates_same_head_and_reopens_draft(self):
        state, snapshot = task(self)
        state.update(mode="manual", published={"head": HEAD, "validation": state["directory"]}, ready_head=HEAD)
        snapshot.update(ci="passed", draft=True)
        current = pull()
        current["draft"] = True
        with mock.patch.object(manual.forge, "snapshot", return_value=snapshot), mock.patch.object(
                manual, "checkout", return_value=HEAD), mock.patch.object(
                manual.publish, "validation") as validation, mock.patch.object(
                manual.publish.forge, "api", return_value=current) as api, mock.patch.object(
                manual.publish, "comment_once") as comment, mock.patch.object(manual.publish, "labels") as labels:
            self.assertEqual(manual.observe(state)["handoff"]["next_action"], "ready")
            manual.ready(state)
            manual.ready(state)
        self.assertEqual(validation.call_count, 2)
        self.assertTrue(any(call.args[0] == "graphql" for call in api.call_args_list))
        self.assertEqual(comment.call_args_list[0], comment.call_args_list[1])
        self.assertEqual(labels.call_args.args, ("owner/repo", 7, True))
        with mock.patch.object(manual.forge, "snapshot", return_value=snapshot), mock.patch.object(
                manual, "checkout", return_value=HEAD), mock.patch.object(
                manual.publish, "validation", side_effect=ValueError("Report changed")), mock.patch.object(
                manual.publish.forge, "api") as api:
            with self.assertRaisesRegex(ValueError, "Report changed"):
                manual.ready(state)
        api.assert_not_called()
