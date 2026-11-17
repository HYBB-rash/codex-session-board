import copy
from contextlib import nullcontext
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
        self.checker = {"commit": "checker", "tree": "checker-tree", "fingerprint": "checker-fingerprint"}
        self.real_checker_identity = pr_queue.checker_identity
        self.checker_patch = mock.patch.object(pr_queue, "checker_identity", return_value=self.checker)
        self.checker_patch.start()
        self.addCleanup(self.checker_patch.stop)
        self.state = {
            "repository": "owner/repo", "project": str(self.project), "base": self.base,
            "prefix": self.base, "checker": self.checker, "items": [], "events": [],
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
        self.assertEqual(initialized["checker"], self.checker)
        self.assertEqual(initialized["items"][0]["status"], "submitted")
        invalid = copy.deepcopy(original)
        invalid["items"][0]["dependencies"] = [2]
        self.write_json(manifest, invalid)
        with self.assertRaisesRegex(ValueError, "Dependencies"):
            pr_queue.initialize(manifest, self.directory / "bad-dependency")
        self.write_json(manifest, original)
        with mock.patch.object(pr_queue, "ROOT", self.project), \
                self.assertRaisesRegex(ValueError, "independent checker"):
            pr_queue.initialize(manifest, self.directory / "same-worktree")
        (self.project / "README.md").write_text("dirty")
        with self.assertRaisesRegex(ValueError, "clean"):
            pr_queue.initialize(manifest, self.directory / "dirty")

    def test_merged_queue_refreshes_publication_without_reopening_merge_or_acceptance(self):
        from scripts import main_release
        self.state.update(batch_merged=True, rehearsal=False, release_status="pending",
                          frozen_candidate={"candidate": self.base})
        with mock.patch.object(main_release, "status", return_value={
                "release_status": "covered", "covered_by": "newer"}) as status:
            pr_queue.refresh_release(self.state)
        status.assert_called_once_with("owner/repo", self.base)
        self.assertTrue(self.state["batch_merged"])
        self.assertEqual(self.state["release_status"], "covered")
        self.assertIn("release_checked_at", self.state)
        self.state["rehearsal"] = True
        with mock.patch.object(main_release, "status") as status:
            pr_queue.refresh_release(self.state)
        status.assert_not_called()

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

    def test_github_feedback_mode_leaves_comment_without_native_dispatch(self):
        self.state["feedback_mode"] = "github"
        item = self.item(1, self.base)
        with mock.patch.object(pr_queue, "comment", return_value="comment-url") as comment, mock.patch.object(
                pr_queue.subprocess, "run") as transport:
            pr_queue.reject(self.state, item, self.queue, "conflict", "author fixes it")
            pr_queue.notify(self.state, item, self.queue)
        comment.assert_called_once()
        transport.assert_not_called()
        self.assertEqual(item["feedback"]["notification"], "github_comment_posted")
        self.assertNotIn("receipt", item["feedback"])

    def test_github_requeue_needs_fresh_evidence_but_no_native_ack(self):
        self.state["feedback_mode"] = "github"
        item = self.item(1, self.base)
        item.update(status="needs-fix", feedback={"notification": "github_comment_posted"})
        self.state["items"] = [item]
        source = self.branch("github-repair", "other.txt", "repair\n")
        incoming = {"number": 1, "head": source, "review": "new-review", "validation": "new-validation"}
        with mock.patch.object(pr_queue, "notify"), mock.patch.object(
                pr_queue, "verify_entry", side_effect=ValueError("stale evidence")):
            with self.assertRaisesRegex(ValueError, "stale evidence"):
                pr_queue.requeue(self.state, incoming, self.queue)
        self.assertEqual(item["status"], "needs-fix")
        with mock.patch.object(pr_queue, "notify"), mock.patch.object(pr_queue, "verify_entry"):
            pr_queue.requeue(self.state, incoming, self.queue)
        self.assertEqual(item["head"], source)
        self.assertEqual(item["status"], "submitted")

    def test_unrejected_new_head_requires_fresh_evidence_without_invented_feedback(self):
        self.state["feedback_mode"] = "github"
        source = self.branch("new-revision", "revision.txt", "updated\n")
        incoming = {"number": 1, "head": source, "review": "new-review", "validation": "new-validation"}
        for status in ("submitted", "accepted", "blocked-dependency"):
            with self.subTest(status=status):
                item = self.item(1, self.base)
                item["status"] = status
                self.state["items"] = [item]
                with mock.patch.object(pr_queue, "notify") as notify, mock.patch.object(
                        pr_queue, "verify_entry", side_effect=ValueError("stale evidence")):
                    with self.assertRaisesRegex(ValueError, "stale evidence"):
                        pr_queue.requeue(self.state, incoming, self.queue)
                self.assertEqual(item["head"], self.base)
                self.assertEqual(item["status"], status)
                notify.assert_not_called()
                with mock.patch.object(pr_queue, "notify") as notify, \
                        mock.patch.object(pr_queue, "verify_entry") as verify:
                    pr_queue.requeue(self.state, incoming, self.queue)
                self.assertEqual(verify.call_args.args[1]["head"], source)
                self.assertEqual(verify.call_args.args[1]["review"], "new-review")
                self.assertEqual(item["status"], "submitted")
                self.assertNotIn("feedback", item)
                self.assertEqual(item.get("recombine_required", False), status == "accepted")
                notify.assert_not_called()
        item = self.item(1, self.base)
        item["status"] = "accepted"
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "verify_entry"), mock.patch.object(
                pr_queue, "commit", side_effect=lambda project, message: self.commit()):
            pr_queue.requeue(self.state, incoming, self.queue)
            self.assertTrue(item["recombine_required"])
            pr_queue.attempt(self.state, self.queue)
        self.assertEqual(item["status"], "accepted")
        self.assertNotIn("recombine_required", item)
        self.assertEqual(self.git("show", f"{self.state['prefix']}:revision.txt"), "updated")
        for status in ("merged", "closed", "accepted"):
            item = self.item(1, source if status == "accepted" else self.base)
            item["status"] = status
            self.state["items"] = [item]
            with self.subTest(terminal_or_unchanged=status), self.assertRaisesRegex(
                    ValueError, "changed source"):
                pr_queue.requeue(self.state, incoming, self.queue)

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
                  "checker": copy.deepcopy(self.checker), "project": str(self.project), "steps": [],
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
        item["repair_instructions"] = "Reproduce from the original failure transcript."
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify"):
            pr_queue.reject(self.state, item, self.queue, "merge", "conflict")
            original = item["feedback"]
            original["comment"] = "existing-comment"
            pr_queue.reject(self.state, item, self.queue, "merge", "conflict")
        self.assertIs(item["feedback"], original)
        self.assertEqual(len(item["attempts"]), 1)
        self.assertEqual(item["feedback"]["comment"], "existing-comment")
        self.assertIn(str(pr_queue.ROOT), original["body"])
        self.assertIn("--scope pr --project", original["body"])
        self.assertIn(item["repair_instructions"], original["body"])
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
                    mock.patch.object(pr_queue, "validate_freeze", return_value="0.3.91"), \
                    mock.patch.object(pr_queue, "notify") as notify, \
                    mock.patch.object(pr_queue.subprocess, "run",
                                      side_effect=subprocess.CalledProcessError(1, "integrate")), \
                    (self.assertRaises(subprocess.CalledProcessError) if should_notify else nullcontext()):
                pr_queue.finish(self.state, self.queue)
            self.assertEqual(item["status"], "accepted")
            self.assertEqual(notify.call_count, int(should_notify))
            self.assertIn("candidate_failed" if should_notify else "candidate_pending",
                          self.state["events"][-1])
        self.assertEqual(len(self.state["events"]), 2)

    def test_requeue_cannot_alias_the_old_head_as_a_new_revision(self):
        item = self.item(1, self.base)
        item["status"] = "needs-fix"
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify") as notify, \
                self.assertRaisesRegex(ValueError, "changed source"):
            pr_queue.requeue(self.state, {"number": 1, "head": "HEAD"}, self.queue)
        notify.assert_not_called()

    def test_runner_failure_retains_candidate_without_author_feedback(self):
        self.state["rehearsal"] = False
        item = self.item(1, self.base)
        item["status"] = "accepted"
        self.state["items"] = [item]
        output = self.queue / f"candidate-{self.base}"
        output.mkdir()
        self.write_json(output / "integration.json", {
            "run": 42, "run_attempt": 1, "merge_status": "infrastructure-blocked",
            "last_error": {"detail": "Candidate run 42 finished: failure"},
        })
        with mock.patch.object(pr_queue, "clean"), \
                mock.patch.object(pr_queue, "git", return_value=self.base), \
                mock.patch.object(pr_queue, "validate_freeze", return_value="0.3.91"), \
                mock.patch.object(pr_queue, "notify") as notify, \
                mock.patch.object(pr_queue.subprocess, "run",
                                  side_effect=subprocess.CalledProcessError(2, "integrate")), \
                self.assertRaises(subprocess.CalledProcessError):
            pr_queue.finish(self.state, self.queue)
        notify.assert_not_called()
        self.assertEqual(self.state["merge_status"], "infrastructure-blocked")
        self.assertEqual(self.state["frozen_candidate"]["status"], "infrastructure-blocked")
        self.assertTrue(Path(self.state["frozen_candidate"]["failed_run"]["report"]).exists())

    def test_child_initialization_error_is_retained_even_without_a_report(self):
        item = self.item(1, self.base)
        item["status"] = "accepted"
        self.state["items"] = [item]
        self.state["reference"] = "refs/heads/codex/rehearsal-main-test"
        def fail(arguments, **options):
            options["stdout"].write("fixture: failed before writing integration.json\n")
            raise subprocess.CalledProcessError(2, arguments)
        with mock.patch.object(pr_queue, "clean"), \
                mock.patch.object(pr_queue, "git", return_value=self.base), \
                mock.patch.object(pr_queue, "validate_freeze", return_value="0.3.91"), \
                mock.patch.object(pr_queue.subprocess, "run", side_effect=fail), \
                self.assertRaises(subprocess.CalledProcessError):
            pr_queue.finish(self.state, self.queue)
        transcript = Path(self.state["integration"]["log"]).read_text()
        self.assertIn("failed before writing integration.json", transcript)
        self.assertIn("failed before writing integration.json", self.state["integration"]["error"])
        self.assertEqual(self.state["merge_status"], "infrastructure-blocked")
        self.assertNotIn("candidate_failed", self.state["events"][-1])

    def test_rehearsal_only_passes_the_restricted_reference_to_integrate(self):
        item = self.item(1, self.base)
        item["status"] = "accepted"
        blocked = self.item(2, self.base)
        blocked["status"] = "needs-fix"
        self.state["items"] = [item, blocked]
        self.state["reference"] = "refs/heads/codex/rehearsal-main-test"
        def integrate(arguments, **options):
            output = self.queue / f"candidate-{self.base}"
            output.mkdir()
            self.write_json(output / "integration.json", {
                "merge_status": "merged", "source": {"commit": self.base},
            })
        with mock.patch.object(pr_queue, "clean"), \
                mock.patch.object(pr_queue, "git", return_value=self.base), \
                mock.patch.object(pr_queue, "validate_freeze", return_value="0.3.91"), \
                mock.patch.object(pr_queue.subprocess, "run", side_effect=integrate) as run:
            pr_queue.finish(self.state, self.queue)
        command = run.call_args.args[0]
        self.assertEqual(command[-2:], ["--reference", self.state["reference"]])
        self.assertEqual(item["status"], "merged")
        self.assertTrue(self.state["batch_merged"])
        self.assertFalse(self.state["batch_delivered"])
        self.assertEqual(self.state["release_status"], "pending")
        self.assertFalse(self.state["all_items_merged"])
        self.assertEqual(self.state["initial_base"], self.base)

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

    def test_preview_reports_conflicts_and_missing_evidence_together(self):
        first = self.branch("first", "README.md", "first\n")
        second = self.branch("second", "README.md", "second\n")
        self.git("merge", "--no-ff", "-m", "prefix", first)
        self.state["prefix"] = self.git("rev-parse", "HEAD")
        item = self.item(2, second)
        self.state["items"] = [item]
        with mock.patch.object(pr_queue, "notify"):
            pr_queue.attempt(self.state, self.queue)
        self.assertEqual(item["status"], "needs-fix")
        self.assertEqual(item["feedback"]["files"], ["README.md"])
        self.assertIn("Mechanical conflicts", item["feedback"]["detail"])
        self.assertIn("Delivery:", item["feedback"]["detail"])
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.git("rev-parse", "HEAD"), self.state["prefix"])

    def test_delivery_evidence_can_be_repaired_without_an_empty_commit(self):
        from scripts import pr_task_github, pr_task_publish
        identifier = "12345678-1234-1234-1234-123456789abc"
        body = f"<!-- pr-queue:{identifier} -->\nPR head: `{self.base}`\nMissing evidence"
        item = self.item(1, self.base)
        item.update(status="needs-fix", feedback={"kind": "delivery", "notification": "acknowledged",
                    "id": identifier, "comment": "https://github.com/owner/repo/pull/1#issuecomment-123"})
        self.state["items"] = [item]
        incoming = {"number": 1, "head": self.base, "review": "new-review", "validation": "new-report"}
        order = []
        with mock.patch.object(pr_queue, "notify"), mock.patch.object(
                pr_queue, "verify_entry", side_effect=lambda *arguments: order.append("verify")) as verify, \
                mock.patch.object(pr_task_github, "api", return_value={"body": body}), \
                mock.patch.object(pr_task_publish, "comment_once",
                                  side_effect=lambda *arguments: order.append("resolved")) as resolve:
            pr_queue.requeue(self.state, incoming, self.queue)
        verify.assert_called_once()
        self.assertEqual(item["status"], "submitted")
        self.assertEqual(item["head"], self.base)
        self.assertEqual(order, ["verify", "resolved"])
        self.assertIn(hashlib.sha256(body.encode()).hexdigest(), resolve.call_args.args[2])

    def test_repair_cannot_narrow_the_fixed_validation_base(self):
        item = self.item(1, self.base)
        self.state["items"] = [item]
        replacement = self.branch("repair", "repair.txt", "repair")
        with mock.patch.object(pr_queue, "notify") as notify, \
                self.assertRaisesRegex(ValueError, "comparison base"):
            pr_queue.requeue(self.state, {"number": 1, "head": replacement, "base": replacement}, self.queue)
        notify.assert_not_called()

    def test_checker_identity_rejects_dirty_tree_and_same_commit_fingerprint_drift(self):
        with mock.patch.object(pr_queue, "ROOT", self.project):
            clean = self.real_checker_identity()
            (self.project / "README.md").write_text("dirty checker")
            with self.assertRaisesRegex(ValueError, "remain clean"):
                self.real_checker_identity()
        self.state["checker"] = clean
        for field in ("commit", "tree", "fingerprint"):
            changed = {**clean, field: "different"}
            with self.subTest(field=field), \
                    mock.patch.object(pr_queue, "checker_identity", return_value=changed), \
                    self.assertRaisesRegex(ValueError, "identity changed"):
                pr_queue.verify_checker(self.state)

    def release_fixture(self):
        (self.project / "Cargo.toml").write_text('[package]\nname = "fixture"\nversion = "0.3.90"\n')
        (self.project / "Cargo.lock").write_text('version = 3\n[[package]]\nname = "fixture"\nversion = "0.3.90"\n')
        (self.project / ".codex-plugin").mkdir()
        for path in ("plugin.json", ".codex-plugin/plugin.json"):
            self.write_json(self.project / path, {"name": "fixture", "version": "0.3.90"})
        self.write_json(self.project / "project-rules.json", {
            "versioning": {"release_only": True, "rollover_after": self.base, "feature_releases": []},
        })
        self.base = self.commit()
        self.state.update(base=self.base, prefix=self.base, initial_base=self.base, rehearsal=False)
        return self.base

    def freeze(self, version, target=None):
        prefix = self.git("rev-parse", "HEAD")
        old = pr_queue.tomllib.loads((self.project / "Cargo.toml").read_text())["package"]["version"]
        for path in ("Cargo.toml", "Cargo.lock"):
            source = self.project / path
            source.write_text(source.read_text().replace(f'"{old}"', f'"{version}"'))
        for path in ("plugin.json", ".codex-plugin/plugin.json"):
            source = self.project / path
            self.write_json(source, {**json.loads(source.read_text()), "version": version})
        rules = self.project / "project-rules.json"
        content = json.loads(rules.read_text())
        content["versioning"]["feature_releases"].append({
            "after_commit": prefix, "from": old, "to": version, "decision": "冻结发布候选",
        })
        self.write_json(rules, content)
        if target is not None:
            (self.project / "context").mkdir(exist_ok=True)
            self.write_json(self.project / "context/integration-target.json", target)
        return self.commit()

    def test_freeze_accepts_only_the_exact_release_fields_and_rehearsal_target(self):
        prefix = self.release_fixture()
        reference = "refs/heads/codex/rehearsal-main-test"
        target = {"reference": reference, "base": prefix}
        candidate = self.freeze("0.3.91", target)
        self.assertEqual(pr_queue.validate_freeze(self.project, prefix, candidate, target), "0.3.91")
        with self.assertRaisesRegex(ValueError, "outside release"):
            pr_queue.validate_freeze(self.project, prefix, candidate)
        with self.assertRaisesRegex(ValueError, "target differs"):
            pr_queue.validate_freeze(self.project, prefix, candidate, {**target, "base": candidate})
        manifest = self.project / "plugin.json"
        self.write_json(manifest, {**json.loads(manifest.read_text()), "mcpServers": "unaudited"})
        self.git("add", ".")
        self.git("commit", "--amend", "-qm", "tampered freeze")
        with self.assertRaisesRegex(ValueError, "Plugin manifest"):
            pr_queue.validate_freeze(self.project, prefix, self.git("rev-parse", "HEAD"), target)

    def test_freeze_rejects_unreviewed_code_and_intermediate_commits(self):
        prefix = self.release_fixture()
        (self.project / "README.md").write_text("unreviewed implementation")
        candidate = self.freeze("0.3.91")
        with self.assertRaisesRegex(ValueError, "outside release"):
            pr_queue.validate_freeze(self.project, prefix, candidate)
        (self.project / "README.md").write_text("base\n")
        self.commit()
        with self.assertRaisesRegex(ValueError, "directly after"):
            pr_queue.validate_freeze(self.project, prefix, self.git("rev-parse", "HEAD"))

    def test_failed_frozen_candidate_accepts_author_repair_and_a_new_version(self):
        prefix = self.release_fixture()
        item = self.item(1, prefix)
        item["status"] = "accepted"
        self.state["items"] = [item]
        first_candidate = self.freeze("0.3.91")
        actual_run = subprocess.run
        outputs = []

        def integrate(arguments, **options):
            if len(arguments) < 2 or not str(arguments[1]).endswith("/integrate_candidate.py"):
                return actual_run(arguments, **options)
            output = Path(arguments[arguments.index("--output") + 1])
            outputs.append(output)
            output.mkdir()
            if len(outputs) == 1:
                self.write_json(output / "integration.json", {
                    "run": 42, "run_attempt": 1,
                    "last_error": {"detail": "Candidate run 42 finished: failure"},
                })
                raise subprocess.CalledProcessError(1, arguments)
            self.write_json(output / "integration.json", {
                "merge_status": "merged", "source": {"commit": self.git("rev-parse", "HEAD")},
            })
            return subprocess.CompletedProcess(arguments, 0)

        def acknowledge(state, source, directory):
            source["feedback"]["notification"] = "acknowledged"

        with mock.patch.object(pr_queue.subprocess, "run", side_effect=integrate), \
                mock.patch.object(pr_queue, "notify", side_effect=acknowledge), \
                mock.patch.object(pr_queue, "verify_entry"), \
                mock.patch.object(pr_queue, "commit", side_effect=lambda project, message: self.commit()):
            with self.assertRaises(subprocess.CalledProcessError):
                pr_queue.finish(self.state, self.queue)
            self.assertEqual(self.state["frozen_candidate"]["reviewed_prefix"], prefix)
            failure = self.state["frozen_candidate"]["failed_run"]
            self.assertEqual(json.loads(Path(failure["report"]).read_text())["run_attempt"], 1)
            with self.assertRaisesRegex(ValueError, "author repair"):
                pr_queue.finish(self.state, self.queue)
            source = self.branch("author-repair", "README.md", "repaired implementation")
            pr_queue.requeue(self.state, {
                "number": 1, "head": source, "review": "review", "validation": "validation",
            }, self.queue)
            pr_queue.attempt(self.state, self.queue)
            repaired_prefix = self.state["prefix"]
            self.assertNotEqual(repaired_prefix, first_candidate)
            second_candidate = self.freeze("0.3.92")
            pr_queue.finish(self.state, self.queue)
        self.assertEqual(item["status"], "merged")
        self.assertEqual(self.state["frozen_candidate"]["candidate"], second_candidate)
        self.assertEqual(self.state["frozen_candidate"]["reviewed_prefix"], repaired_prefix)
        self.assertEqual(self.state["frozen_candidate"]["version"], "0.3.92")
        self.assertEqual(len(set(outputs)), 2)
        self.assertTrue(self.state["all_items_merged"])
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_infrastructure_retry_requires_a_new_successful_attempt_of_the_identical_source(self):
        prefix = self.release_fixture()
        candidate = self.freeze("0.3.91")
        self.state["prefix"] = candidate
        self.state["frozen_candidate"] = {
            "candidate": candidate, "reviewed_prefix": prefix, "status": "failed",
            "failed_run": {"id": 42, "attempt": 1, "report": "original-failure.json"},
        }
        record = {"id": 42, "head_sha": candidate, "run_attempt": 1,
                  "status": "completed", "conclusion": "success"}
        for changes in ({}, {"run_attempt": 2, "head_sha": prefix}, {"run_attempt": 2, "conclusion": "failure"}):
            with self.subTest(changes=changes), \
                    mock.patch.object(pr_queue, "github", return_value=json.dumps({**record, **changes})), \
                    self.assertRaisesRegex(ValueError, "newer attempt succeeds"):
                pr_queue.retry_candidate(self.state, self.queue)
        with mock.patch.object(pr_queue, "github", return_value=json.dumps({**record, "run_attempt": 2})), \
                mock.patch.object(pr_queue, "finish") as finish:
            pr_queue.retry_candidate(self.state, self.queue)
        finish.assert_called_once_with(self.state, self.queue)
        self.assertEqual(self.state["frozen_candidate"]["status"], "retry-ready")
        self.assertEqual(self.state["frozen_candidate"]["failed_run"]["attempt"], 1)
        self.assertEqual(self.state["events"][-1]["run_attempt"], 2)
