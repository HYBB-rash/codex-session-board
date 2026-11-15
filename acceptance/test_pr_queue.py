import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import pr_queue


class PRQueueTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.project = self.directory / "project"
        self.project.mkdir()
        self.git("init", "-qb", "codex/base")
        self.git("config", "user.name", "Queue fixture")
        self.git("config", "user.email", "queue@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.hooksPath", "/dev/null")
        (self.project / "README.md").write_text("base\n")
        (self.project / "acceptance").mkdir()
        self.test_name = "acceptance.test_fixture.FixtureTests.test_case"
        self.write_json(self.project / "acceptance/bindings.json", {
            "fingerprint_paths": ["README.md"], "requirements": {"DELIVERY-001": [self.test_name]},
        })
        self.base = self.commit()
        self.state = {
            "repository": "owner/repo", "project": str(self.project), "base": self.base,
            "prefix": self.base, "checker": "checker", "items": [], "events": [],
            "started_at": pr_queue.now(), "rehearsal": True,
        }
        self.queue = self.directory / "queue"
        self.queue.mkdir()

    def git(self, *arguments):
        return subprocess.check_output(["git", *arguments], cwd=self.project,
                                       text=True, stderr=subprocess.PIPE).strip()

    def write_json(self, path, value):
        path.write_text(json.dumps(value))

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def branch(self, name, filename, value):
        self.git("switch", "-qc", name, self.base)
        (self.project / filename).write_text(value)
        source = self.commit()
        self.git("switch", "-q", "codex/base")
        return source

    def item(self, number, source, dependencies=()):
        return {"number": number, "head": source, "base": self.base,
                "route": {"thread_id": f"author-{number}", "repair_cwd": str(self.project)},
                "dependencies": list(dependencies), "review": "missing", "validation": "missing",
                "status": "submitted", "attempts": []}

    def test_initialization_rejects_dirty_sources_and_forward_dependencies(self):
        manifest = self.directory / "manifest.json"
        original = {**self.state, "items": [self.item(1, self.base)]}
        self.write_json(manifest, original)
        initialized = pr_queue.initialize(manifest, self.directory / "initialized")
        self.assertEqual(initialized["prefix"], self.base)
        self.assertEqual(initialized["items"][0]["status"], "submitted")
        invalid = copy.deepcopy(original)
        invalid["items"][0]["dependencies"] = [2]
        self.write_json(manifest, invalid)
        with self.assertRaisesRegex(ValueError, "Dependencies"):
            pr_queue.initialize(manifest, self.directory / "bad-dependency")
        self.write_json(manifest, original)
        (self.project / "README.md").write_text("dirty")
        with self.assertRaisesRegex(ValueError, "clean"):
            pr_queue.initialize(manifest, self.directory / "dirty")

    def test_conflict_rolls_back_and_independent_pr_continues(self):
        first = self.branch("first", "README.md", "first\n")
        conflict = self.branch("conflict", "README.md", "conflict\n")
        independent = self.branch("independent", "other.txt", "independent\n")
        self.state["items"] = [
            self.item(1, first), self.item(2, conflict),
            self.item(3, independent), self.item(4, conflict, [2]),
        ]
        with mock.patch.object(pr_queue, "verify_entry"), mock.patch.object(pr_queue, "notify"), \
                mock.patch.object(pr_queue, "commit", side_effect=lambda project, message: self.commit()):
            pr_queue.attempt(self.state, self.queue)
        self.assertEqual([item["status"] for item in self.state["items"]],
                         ["accepted", "needs-fix", "accepted", "blocked-dependency"])
        self.assertEqual(self.state["items"][1]["feedback"]["files"], ["README.md"])
        self.assertEqual((self.project / "README.md").read_text(), "first\n")
        self.assertEqual((self.project / "other.txt").read_text(), "independent\n")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.state["items"][1]["feedback"]["prefix"], self.state["items"][0]["ingress"])

    def test_commit_failure_aborts_only_the_uncommitted_merge(self):
        source = self.branch("source", "other.txt", "source\n")
        self.state["items"] = [self.item(1, source)]
        with mock.patch.object(pr_queue, "verify_entry"), \
                mock.patch.object(pr_queue, "commit", side_effect=RuntimeError("hook failed")), \
                self.assertRaisesRegex(RuntimeError, "hook failed"):
            pr_queue.attempt(self.state, self.queue)
        self.assertEqual(self.git("rev-parse", "HEAD"), self.base)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.state["items"][0]["status"], "submitted")

    def test_transport_requires_exact_native_turn_and_preserves_first_failure(self):
        item = self.item(1, self.base)
        self.state["items"] = [item]
        replies = ["wrong-thread", "waiting_idle", "sent", "acknowledged"]
        commands = []

        def transport(arguments, **options):
            commands.append(arguments[2])
            request = json.loads(Path(arguments[arguments.index("--request") + 1]).read_text())
            phase = replies.pop(0)
            receipt = {
                "thread_id": "wrong" if phase == "wrong-thread" else "author-1",
                "request_id": request["id"], "turn_id": "native-turn",
                "body_sha256": hashlib.sha256(request["body"].encode()).hexdigest(),
                "state": "sent" if phase == "wrong-thread" else phase,
            }
            if phase == "acknowledged":
                receipt["acknowledged_at"] = pr_queue.now()
            self.write_json(Path(arguments[arguments.index("--output") + 1]), receipt)
            return subprocess.CompletedProcess(arguments, 0, "", "")

        with mock.patch.object(pr_queue, "comment", return_value="https://example.invalid/comment") as comment, \
                mock.patch.object(pr_queue.subprocess, "run", side_effect=transport):
            pr_queue.reject(self.state, item, self.queue, "merge", "conflict", ["README.md"])
            self.assertEqual(item["feedback"]["notification"], "pending")
            pr_queue.notify(self.state, item, self.queue)
            self.assertEqual(item["feedback"]["notification"], "waiting_idle")
            pr_queue.notify(self.state, item, self.queue)
            self.assertEqual(item["feedback"]["notification"], "sent")
            pr_queue.notify(self.state, item, self.queue)
            self.assertEqual(item["feedback"]["notification"], "acknowledged")
        self.assertEqual(comment.call_count, 1)
        self.assertEqual(commands, ["send", "poll", "send", "poll"])
        self.assertEqual(len(item["feedback"]["errors"]), 1)
        self.assertEqual(item["attempts"][0]["id"], item["feedback"]["id"])

    def test_comment_retry_reads_existing_marker_without_duplicate_post(self):
        feedback = {"id": "request", "head": self.base, "prefix": self.base,
                    "detail": "conflict", "files": ["README.md"]}
        existing = [[{"body": "<!-- pr-queue:request -->", "html_url": "comment-url"}]]
        with mock.patch.object(pr_queue, "github", return_value=json.dumps(existing)) as github:
            self.assertEqual(pr_queue.comment(self.state, self.item(1, self.base), feedback), "comment-url")
        self.assertEqual(github.call_count, 1)
        self.assertNotIn("POST", github.call_args.args)

    def test_requeue_requires_author_acknowledgement_and_new_head_evidence(self):
        item = self.item(1, self.base)
        item.update(status="needs-fix", feedback={"notification": "sent"}, attempts=[{"failure": "retained"}])
        self.state["items"] = [item]
        source = self.branch("repair", "other.txt", "repair\n")
        incoming = {"number": 1, "head": source, "review": "new-review", "validation": "new-validation"}
        with mock.patch.object(pr_queue, "notify"), mock.patch.object(pr_queue, "verify_entry") as verify:
            with self.assertRaisesRegex(ValueError, "not acknowledged"):
                pr_queue.requeue(self.state, incoming, self.queue)
            verify.assert_not_called()
            item["feedback"]["notification"] = "acknowledged"
            pr_queue.requeue(self.state, incoming, self.queue)
        self.assertEqual(item["head"], source)
        self.assertEqual(item["status"], "submitted")
        self.assertEqual(item["attempts"], [{"failure": "retained"}])
        self.assertNotIn("feedback", item)
        self.assertEqual(verify.call_args.args[1]["review"], "new-review")

    def evidence(self):
        (self.project / "README.md").write_text("updated\n")
        source = self.commit()
        item = self.item(1, source)
        directory = self.directory / "validation"
        directory.mkdir()
        review = self.directory / "review.json"
        self.write_json(review, {"head": source, "passed": True, "reviewer": "independent-reviewer"})
        item.update(validation=str(directory), review=str(review))
        identity = pr_queue.identity(self.project)
        strategy = pr_queue.pr_plan.plan(self.project, self.base)
        tests = {"scope": "pr_tests", "source": identity, "plan": strategy, "passed": True,
                 "release_ready": False, "inventory": [self.test_name], "selected": [self.test_name],
                 "not_run": [], "protocol": 1, "results": [{"id": self.test_name, "status": "pass"}]}
        self.write_json(directory / "TESTS.json", tests)
        record = {"scope": "pr", "passed": True, "source": identity, "plan": strategy,
                  "release_ready": False, "acceptance_status": "not_performed",
                  "checker": {"commit": "checker"}, "project": str(self.project), "steps": [],
                  "tests_sha256": hashlib.sha256((directory / "TESTS.json").read_bytes()).hexdigest()}
        for name in ("project-check", "all-target-clippy", "source-package", "related-tests"):
            (directory / f"{name}.log").write_text("passed\n")
            record["steps"].append({"name": name, "returncode": 0,
                                    "log_sha256": hashlib.sha256(b"passed\n").hexdigest()})
        self.write_json(directory / "VALIDATION.json", record)
        return item, record

    def test_delivery_gate_rejects_stale_review_source_checker_and_logs(self):
        item, original = self.evidence()
        pr_queue.verify_entry(self.state, item)
        for mutation in ("source", "checker", "step"):
            changed = copy.deepcopy(original)
            if mutation == "step":
                changed["steps"].pop()
            else:
                changed[mutation]["commit"] = "stale"
            self.write_json(Path(item["validation"]) / "VALIDATION.json", changed)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                pr_queue.verify_entry(self.state, item)
        self.write_json(Path(item["validation"]) / "VALIDATION.json", original)
        self.write_json(Path(item["review"]), {"head": item["head"], "passed": True, "reviewer": "author-1"})
        with self.assertRaisesRegex(ValueError, "independent"):
            pr_queue.verify_entry(self.state, item)
        self.write_json(Path(item["review"]), {"head": item["head"], "passed": True, "reviewer": "other"})
        (Path(item["validation"]) / "related-tests.log").write_text("changed")
        with self.assertRaisesRegex(ValueError, "transcript"):
            pr_queue.verify_entry(self.state, item)

    def test_rehearsal_cannot_promote_the_real_main(self):
        with mock.patch.object(pr_queue.subprocess, "run") as run, self.assertRaisesRegex(ValueError, "permitted"):
            pr_queue.finish(self.state, self.queue)
        run.assert_not_called()

    def test_identical_feedback_reuses_the_original_request_and_attempt(self):
        item = self.item(1, self.base)
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify"):
            pr_queue.reject(self.state, item, self.queue, "merge", "conflict")
            original = item["feedback"]
            original["comment"] = "existing-comment"
            pr_queue.reject(self.state, item, self.queue, "merge", "conflict")
        self.assertIs(item["feedback"], original)
        self.assertEqual(len(item["attempts"]), 1)
        self.assertEqual(item["feedback"]["comment"], "existing-comment")
        self.state["started_at"] = pr_queue.now()
        next_item = self.item(1, self.base)
        self.state["items"] = [next_item]
        with mock.patch.object(pr_queue, "notify"):
            pr_queue.reject(self.state, next_item, self.queue, "merge", "conflict")
        self.assertNotEqual(next_item["feedback"]["id"], original["id"])

    def test_candidate_failure_notifies_authors_but_pending_ci_does_not(self):
        self.state["rehearsal"] = False
        item = self.item(1, self.base)
        self.state["items"] = [item]
        output = self.queue / f"candidate-{self.base}"
        output.mkdir()
        for detail, should_notify in (
                ("Candidate is still pending; resume this same output directory", False),
                ("Candidate run 42 finished: failure", True)):
            item["status"] = "accepted"
            self.write_json(output / "integration.json", {"last_error": {"detail": detail}})
            with self.subTest(detail=detail), mock.patch.object(pr_queue, "clean"), \
                    mock.patch.object(pr_queue, "git", return_value=self.base), \
                    mock.patch.object(pr_queue, "notify") as notify, \
                    mock.patch.object(pr_queue.subprocess, "run", side_effect=[
                        subprocess.CompletedProcess([], 0), subprocess.CalledProcessError(1, "integrate"),
                    ]), self.assertRaises(subprocess.CalledProcessError):
                pr_queue.finish(self.state, self.queue)
            self.assertEqual(item["status"], "accepted")
            self.assertEqual(notify.call_count, int(should_notify))
        self.assertEqual(len(self.state["events"]), 2)

    def test_requeue_cannot_alias_the_old_head_as_a_new_revision(self):
        item = self.item(1, self.base)
        item["status"] = "needs-fix"
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify") as notify, \
                self.assertRaisesRegex(ValueError, "changed source"):
            pr_queue.requeue(self.state, {"number": 1, "head": "HEAD"}, self.queue)
        notify.assert_not_called()

    def test_rehearsal_only_passes_the_restricted_reference_to_integrate(self):
        item = self.item(1, self.base)
        item["status"] = "accepted"
        self.state["items"] = [item]
        self.state["main_reference"] = "refs/heads/codex/rehearsal-main-test"
        with mock.patch.object(pr_queue, "clean"), \
                mock.patch.object(pr_queue, "git", return_value=self.base), \
                mock.patch.object(pr_queue.subprocess, "run") as run:
            pr_queue.finish(self.state, self.queue)
        command = run.call_args.args[0]
        self.assertEqual(command[-2:], ["--main-reference", self.state["main_reference"]])
        self.assertEqual(item["status"], "merged")

    def test_repair_must_retain_the_original_source_commit(self):
        original = self.branch("original", "original.txt", "original")
        replacement = self.branch("unrelated", "replacement.txt", "replacement")
        item = self.item(1, original)
        item["status"] = "needs-fix"
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify") as notify, \
                self.assertRaises(subprocess.CalledProcessError):
            pr_queue.requeue(self.state, {"number": 1, "head": replacement}, self.queue)
        notify.assert_not_called()
