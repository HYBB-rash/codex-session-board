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
from scripts.pr_task_state import persist, save


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
