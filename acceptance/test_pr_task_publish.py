import json
from pathlib import Path
import subprocess
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import HEAD, pull, saved, task
from acceptance.pr_task_runtime_fixture import runtime
from scripts import pr_task as cli
from scripts import pr_task_init as setup
from scripts import pr_task_publish as publish
from scripts.pr_task_state import save


class PRTaskPublishTests(unittest.TestCase):
    def test_registration_is_idempotent_and_one_branch_has_one_owner(self):
        old, database, _ = runtime(self)
        project = Path(old["project"])
        head = json.loads(database.read_text())["pr"]["head"]["sha"]
        subprocess.run(["git", "update-ref", "refs/remotes/origin/main", head],
                       cwd=project, check=True, capture_output=True)
        arguments = cli.parser().parse_args([
            "init", "--state", str(Path(old["directory"]) / "registered"),
            "--project", str(project), "--repository", "owner/repo",
            "--thread", old["route"]["thread_id"], "--socket", "/fixture.sock",
        ])
        def api(endpoint):
            return {"login": "owner"} if endpoint == "user" else {"full_name": "owner/repo"}
        with mock.patch.object(setup.forge, "api", side_effect=api), mock.patch.object(
                setup.native, "register", return_value=(old["route"], old["session_anchor"])) as register, mock.patch.object(
                setup, "checker_identity", return_value={"commit": "frozen-checker"}):
            first = setup.initialize(arguments)
            second = setup.initialize(arguments)
            self.assertEqual(first, second)
            self.assertEqual(first["checker"]["commit"], "frozen-checker")
            self.assertEqual(register.call_count, 1)
            arguments.state = Path(old["directory"]) / "duplicate"
            with self.assertRaisesRegex(ValueError, "already has a PR task"):
                setup.initialize(arguments)

    def test_author_cannot_register_self_as_checker_and_changed_checker_is_rejected(self):
        state, _ = task(self)
        arguments = cli.parser().parse_args([
            "init", "--state", state["directory"], "--project", str(setup.ROOT),
            "--repository", "owner/repo", "--thread", state["route"]["thread_id"], "--socket", "/fixture",
        ])
        with self.assertRaisesRegex(ValueError, "separate author worktree"):
            setup.initialize(arguments)
        state["checker"] = {"commit": "frozen"}
        with mock.patch.object(setup, "checker_identity", return_value={"commit": "changed"}):
            with self.assertRaisesRegex(ValueError, "checker changed"):
                setup.verify_checker(state)

    def test_first_submit_validates_before_push_and_creates_draft(self):
        state, _ = task(self)
        state.update(pr=None, phase="developing")
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            if "--output" in command:
                output = Path(command[command.index("--output") + 1])
                output.mkdir()
                save(output / "VALIDATION.json", {"passed": True})
        observed = pull()
        observed["draft"] = True
        with mock.patch.object(publish, "checkout", return_value=HEAD), mock.patch.object(
                publish, "published_pr", return_value=None), mock.patch.object(
                publish.subprocess, "run", side_effect=run), mock.patch.object(
                publish, "validation"), mock.patch.object(
                publish.forge, "api", return_value=observed) as api:
            publish.submit(state, "Title", "Body")
        self.assertIn("--expect-head", calls[0])
        self.assertEqual(calls[1][:2], ["git", "push"])
        self.assertNotIn("--force", calls[1])
        self.assertIn("draft=true", api.call_args_list[0].args)
        self.assertEqual(saved(state)["published"]["head"], HEAD)
        self.assertEqual(state["phase"], "waiting-ci")

    def test_failed_validation_cannot_push_or_create_pr(self):
        state, _ = task(self)
        state.update(pr=None, phase="developing")
        with mock.patch.object(publish, "checkout", return_value=HEAD), mock.patch.object(
                publish, "published_pr", return_value=None), mock.patch.object(
                publish.subprocess, "run", side_effect=subprocess.CalledProcessError(1, ["validate"])) as run, mock.patch.object(
                publish.forge, "api") as api:
            with self.assertRaises(subprocess.CalledProcessError):
                publish.submit(state, "Title", "Body")
        self.assertEqual(run.call_count, 1)
        api.assert_not_called()
        self.assertNotIn("published", saved(state))

    def test_changed_remote_after_validation_prevents_push(self):
        state, _ = task(self)
        state["published"] = {"head": HEAD, "validation": state["directory"]}
        original = pull()
        changed = pull()
        changed["head"]["sha"] = "c" * 40
        with mock.patch.object(publish, "checkout", return_value=HEAD), mock.patch.object(
                publish, "published_pr", side_effect=[original, changed]), mock.patch.object(
                publish, "validation"), mock.patch.object(publish.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "changed during validation"):
                publish.submit(state)
        run.assert_not_called()

    def test_lost_creation_reply_recovers_terminal_pr_and_schedules_idle_cleanup(self):
        state, snapshot = task(self)
        state.update(pr=None, phase="developing")
        closed = pull()
        closed.update(state="closed", merged=True)
        snapshot.update(state="merged", merge_commit="d" * 40)
        arguments = cli.parser().parse_args(["submit", "--state", state["directory"]])
        with mock.patch.object(publish, "checkout", return_value=HEAD), mock.patch.object(
                publish, "published_pr", return_value=closed), mock.patch.object(
                cli.lifecycle.forge, "snapshot", return_value=snapshot), mock.patch.object(
                cli.lifecycle.native, "archive", return_value=False), mock.patch.object(
                cli.scheduler, "install") as install:
            result = cli.execute(state, arguments)
        self.assertEqual(result["phase"], "finalizing")
        self.assertEqual(state["outcome"]["result"], "merged")
        self.assertIn("finalize", result["recovery"])
        install.assert_called_once_with(state)
