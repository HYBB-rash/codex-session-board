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
        registry.mkdir()
        config = self.directory / "duty.json"
        data = {"mode": "production-duty", "repository": duty.PRODUCTION,
                "checker": str(self.candidate), "checker_head": BASE, "registry_project": str(registry),
                "state": str(self.directory / "state")}
        save(config, data)
        with mock.patch.object(duty, "ROOT", self.candidate), \
                mock.patch.object(duty, "git", side_effect=[BASE, "", str(registry)]), \
                mock.patch.object(duty, "verify_origin") as verify_origin:
            configuration, checker, _ = duty.configuration(config, "tick")
        self.assertEqual(checker, self.candidate)
        self.assertEqual(configuration["registry_project"], str(registry))
        verify_origin.assert_called_once_with(registry, duty.PRODUCTION)
        save(config, {**data, "registry_project": "registry"})
        with mock.patch.object(duty, "ROOT", self.candidate), \
                mock.patch.object(duty, "git", side_effect=[BASE, ""]), \
                self.assertRaisesRegex(ValueError, "absolute persistent registry"):
            duty.configuration(config, "tick")

    def test_duty_recovers_queue_initialized_before_registry_write(self):
        registry_project = self.directory / "persistent"
        runtime = self.directory / "runtime"
        runtime.mkdir()
        queue = runtime / "queue-pr-7"
        queue.mkdir()
        state = {"repository": self.repository, "items": [{"number": 7, "status": "submitted"}]}
        claim = {"pr": 7, "repository": self.repository, "queue": None}
        save(queue / "queue.json", state)
        registry_dir = registry_project / ".git" / "pr-queues"
        with mock.patch.object(registry, "registry", return_value=registry_dir) as index, \
                mock.patch.object(duty, "queue_command", return_value=state) as command, \
                mock.patch.object(duty, "verify_queue"):
            self.assertEqual(queue_recovery.locate(registry_project, claim, runtime), queue)
            config = {**self.config, "registry_project": str(registry_project)}
            duty.advance(config, self.candidate, runtime, self.assignment, {"result": "waiting-review"}, queue)
            self.assertEqual(registry.locate(self.repository, 7, registry_project), queue)
        self.assertIn(mock.call(registry_project), index.call_args_list)
        command.assert_called_once_with(self.candidate, "status", queue)

    def test_published_draft_reports_pending_ci_without_admitting_source(self):
        checker = {"commit": BASE}
        author_task = {**self.assignment, "repository": "owner/repo", "base": "main",
                       "branch": "codex/task", "published": {"head": HEAD},
                       "checker": checker, "checker_project": str(self.directory), "checks": ["changes"]}
        assignment = {**self.assignment, "author_task": str(self.directory)}
        with mock.patch.object(duty.forge, "api", return_value={**pull(), "draft": True}), \
                mock.patch.object(duty.forge, "check_state", return_value={"ci": "pending", "items": []}), \
                mock.patch.object(duty, "identity", return_value=checker), \
                mock.patch.object(duty, "load", return_value=author_task):
            observed = duty.observation(self.config, assignment, self.directory)
        self.assertEqual(observed["result"], "waiting-ci")
        self.assertTrue(observed["draft"])
        self.assertNotIn("item", observed)

    def test_resume_uses_the_saved_candidate_and_run(self):
        identifier = digest([self.repository, self.assignment["pr"], HEAD])
        queue = self.directory / f"queue-{identifier}"
        queue.mkdir()
        checker = {"commit": BASE}
        saved = {"repository": self.repository, "project": str(self.candidate), "checker": checker,
                 "feedback_mode": "github", "items": [{"number": 7, "head": HEAD, "status": "accepted"}],
                 "frozen_candidate": {"candidate": BASE}, "integration": {"run": 4321},
                 "handoff": {"result": "waiting-ci"}}
        with mock.patch.object(duty, "identity", return_value=checker), \
                mock.patch.object(duty, "queue_command", side_effect=[saved, saved]) as command:
            report = duty.advance(self.config, self.directory, self.directory, self.assignment,
                                  {"result": "waiting-ci"})
        self.assertEqual(report["integration"]["run"], 4321)
        self.assertEqual(command.call_args_list, [mock.call(self.directory, "status", queue),
                                                  mock.call(self.directory, "resume", queue)])

    def test_release_read_error_preserves_confirmed_merge_for_delivery(self):
        identifier = digest([self.repository, self.assignment["pr"], HEAD])
        queue = self.directory / f"queue-{identifier}"
        queue.mkdir()
        checker = {"commit": BASE}
        saved = {"repository": self.repository, "project": str(self.candidate), "checker": checker,
                 "feedback_mode": "github", "items": [{"number": 7, "head": HEAD, "status": "merged"}],
                 "batch_merged": True, "frozen_candidate": {"candidate": BASE, "status": "merged"},
                 "integration": {"run": 4321}, "handoff": {"result": "merged"},
                 "observation": {"items": [{"result": "merged"}]}}
        with mock.patch.object(duty, "identity", return_value=checker), \
                mock.patch.object(duty, "queue_command",
                                  side_effect=[saved, saved, saved, ValueError("GitHub unavailable")]):
            report = duty.advance(self.config, self.directory, self.directory, self.assignment,
                                  {"result": "closed", "github_merged": True})
        self.assertEqual(report["result"], "merged")
        self.assertEqual(report["candidate"], saved["frozen_candidate"])
        self.assertEqual(report["release_status"], "unknown")

    def test_delayed_delivery_uses_one_request_and_confirms_the_original_turn(self):
        key = [self.repository, 7, HEAD, "merged"]

        def send(rpc, route, request, receipt, path, prompt):
            receipt.update(state="sent", attempted_at="now", turn_id="original-turn")

        def observe(rpc, route, receipt):
            receipt["acknowledged_at"] = "later"

        with mock.patch.object(delivery.transport, "connect_endpoint",
                               side_effect=lambda route: nullcontext(None)), \
                mock.patch.object(delivery.native, "is_archived", return_value=False), \
                mock.patch.object(delivery.transport, "send", side_effect=send) as sender, \
                mock.patch.object(delivery.transport, "observe", side_effect=observe) as observer:
            sent = delivery.deliver(self.directory, self.route, key, "one result")
            confirmed = delivery.deliver(self.directory, self.route, key, "one result")
            self.assertEqual(delivery.deliver(self.directory, self.route, key, "one result"), confirmed)
        self.assertEqual(sent["request_id"], confirmed["request_id"])
        self.assertEqual(confirmed["turn_id"], "original-turn")
        self.assertEqual(confirmed["state"], "delivered")
        sender.assert_called_once()
        observer.assert_called_once()

    def test_automatic_author_receives_failure_in_the_original_repair_event(self):
        author, snapshot = task(self)
        identifier = str(uuid.uuid4())
        comment = {**review(), "body": f"<!-- pr-queue:{identifier} -->\nPR head: `{HEAD}`"}
        items = duty.forge.feedback(pull(), [], [comment], [], {}, author["reviewers"])
        event = pr_task_lifecycle.create_event(author, snapshot, items)
        self.assertTrue(event["prompt"].startswith(f"PR #7 本轮合并失败。反馈 {comment['html_url']}。"))
        assignment = {**self.assignment, "author_task": author["directory"]}
        result = {"result": "needs-author", "feedback": {
            "kind": "merge", "id": identifier, "comment": comment["html_url"],
        }}
        receipt = {"state": "delivered", "turn_id": "original-repair-turn"}
        with mock.patch.object(delivery, "deliver") as send:
            first = delivery.dispatch(self.config, self.directory, assignment, result)
            event["delivery"] = receipt
            persist(author)
            confirmed = delivery.dispatch(self.config, self.directory, assignment, result)
        self.assertEqual(first["author"], {"state": "waiting-for-author-tick"})
        self.assertEqual(confirmed["author"], receipt)
        send.assert_not_called()
