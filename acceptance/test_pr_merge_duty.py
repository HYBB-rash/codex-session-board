from contextlib import nullcontext
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import uuid

from acceptance.pr_task_fixtures import BASE, HEAD, pull, review, task
from scripts import pr_merge_duty as duty
from scripts import pr_merge_duty_delivery as delivery
from scripts import pr_merge_duty_queue as queue_recovery
from scripts import pr_queue_registry as registry
from scripts import pr_task_lifecycle
from scripts.pr_task_state import digest, persist, save


class PRMergeDutyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.candidate = self.directory / "candidate"
        self.candidate.mkdir()
        self.repository = "owner/repo"
        self.config = {"repository": self.repository}
        self.assignment = {"pr": 7, "head": HEAD, "candidate": str(self.candidate)}
        self.route = {"thread_id": "b40e137c-0837-4daa-aa1f-27a8f15b5004"}

    def test_duty_configuration_rejects_production_and_legacy_authorized_mode(self):
        config_path = self.directory / "duty.json"
        for mode, repository in (("isolated", duty.PRODUCTION.upper()),
                                 ("authorized-pr", duty.PRODUCTION),
                                 ("authorized-pr", self.repository)):
            with self.subTest(mode=mode, repository=repository):
                save(config_path, {"mode": mode, "repository": repository})
                with mock.patch.object(duty, "git") as git:
                    with self.assertRaisesRegex(ValueError, "Use isolated mode"):
                        duty.configuration(config_path)
                git.assert_not_called()

    def test_production_scan_reads_its_handoff_and_requires_remote_main(self):
        handoff = self.directory / "handoff.json"
        config = self.directory / "duty.json"
        save(handoff, {"repository": duty.PRODUCTION, "assignments": []})
        save(config, {"mode": "production-scan", "repository": duty.PRODUCTION,
                      "checker": str(self.candidate), "checker_head": BASE,
                      "handoff": str(handoff), "state": str(self.directory / "state")})
        with mock.patch.object(duty, "ROOT", self.candidate), \
                mock.patch.object(duty, "git", side_effect=[BASE, "", BASE, ""]), \
                mock.patch.object(duty, "private_directory", side_effect=lambda path: path), \
                mock.patch.object(duty.forge, "api", side_effect=[
                    {"object": {"sha": BASE}}, {"object": {"sha": HEAD}}]) as api:
            configuration, _, _ = duty.configuration(config)
            self.assertEqual(configuration["assignments"], [])
            with self.assertRaisesRegex(ValueError, "differs from remote main"):
                duty.configuration(config)
        self.assertEqual(api.call_args_list, [mock.call(f"repos/{duty.PRODUCTION}/git/ref/heads/main")] * 2)
        with mock.patch.object(duty.forge, "api") as api:
            with self.assertRaisesRegex(ValueError, "only supports read-only scan"):
                duty.configuration(config, "tick")
            handoff.unlink()
            with self.assertRaises(FileNotFoundError):
                duty.configuration(config)
        api.assert_not_called()

    def test_waiting_review_ci_or_changed_head_never_creates_a_queue(self):
        with mock.patch("scripts.pr_queue_registry.locate", side_effect=FileNotFoundError), \
                mock.patch.object(duty, "git", return_value=str(self.directory)), \
                mock.patch.object(duty, "queue_command") as queue:
            for result in ("waiting-review", "waiting-ci", "needs-user", "waiting-author"):
                observation = {"result": result}
                self.assertEqual(duty.advance(self.config, self.directory, self.directory,
                                              self.assignment, observation), observation)
        queue.assert_not_called()

    def test_production_runtime_and_registry_can_be_separate_git_roots(self):
        registry = self.directory / "registry"
