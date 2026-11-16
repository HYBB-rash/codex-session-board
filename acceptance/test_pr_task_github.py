import copy
import hashlib
import json
import subprocess
import time
import unittest
from unittest import mock
import uuid

from acceptance.pr_task_fixtures import HEAD, pull, review, task
from scripts import pr_task_github as forge


class PRTaskGithubTests(unittest.TestCase):
    def test_pending_review_and_inline_comment_are_not_consumed_before_publication(self):
        parent = review(status="PENDING")
        inline = {**review(2), "pull_request_review_id": 1}
        roots = {2: {"isResolved": False, "isOutdated": False}}
        self.assertEqual(forge.feedback(pull(), [parent], [], [inline], roots, ["owner"]), [])
        parent["state"] = "CHANGES_REQUESTED"
        found = forge.feedback(pull(), [parent], [], [inline], roots, ["owner"])
        self.assertEqual([entry["kind"] for entry in found], ["review", "inline"])

    def test_same_account_structured_feedback_survives_author_filter(self):
        comment = {**review(), "body": f"<!-- pr-task:feedback {uuid.uuid4()} -->\nPR head: `{HEAD}`\nFix"}
        own_receipt = {**review(2), "body": "<!-- pr-task:result id -->\nFixed"}
        stranger = {**comment, "id": 3, "user": {"login": "stranger"}}
        found = forge.feedback(pull(), [], [comment, own_receipt, stranger], [], {}, ["owner"])
        self.assertEqual([entry["id"] for entry in found], ["1"])
        before = found[0]["key"]
        comment["body"] += " updated requirement"
        self.assertNotEqual(forge.feedback(pull(), [], [comment], [], {}, ["owner"])[0]["key"], before)

    def test_revalidated_same_head_feedback_resolves_only_its_exact_original_body(self):
        identifier = str(uuid.uuid4())
        comment = {**review(), "body": f"<!-- pr-queue:{identifier} -->\nPR head: `{HEAD}`\nMissing evidence"}
        digest = hashlib.sha256(comment["body"].encode()).hexdigest()
        resolution = {**review(2), "body": f"<!-- pr-queue:resolved {identifier} {HEAD} {digest} -->"}
        self.assertEqual(forge.feedback(pull(), [], [comment, resolution], [], {}, ["owner"]), [])
        for changed in ({**comment, "body": comment["body"] + " new issue"},
                        {**comment, "body": comment["body"].replace(identifier, str(uuid.uuid4()))}):
            self.assertEqual(len(forge.feedback(pull(), [], [changed, resolution], [], {}, ["owner"])), 1)
        resolution["user"] = {"login": "stranger"}
        self.assertEqual(len(forge.feedback(pull(), [], [comment, resolution], [], {}, ["owner"])), 1)

    def test_old_head_feedback_and_resolved_threads_do_not_wake_author(self):
        comment = {**review(), "body": f"<!-- pr-queue:{uuid.uuid4()} -->\nPR head: `{'c' * 40}`"}
        inline = {**review(2), "pull_request_review_id": 1}
        roots = {2: {"isResolved": True, "isOutdated": False}}
        self.assertEqual(forge.feedback(pull(), [review(status="COMMENTED")], [comment],
                                       [inline], roots, ["owner"]), [])

    def test_later_approval_supersedes_old_request_changes_body(self):
        found = forge.feedback(pull(), [review(), review(2, "APPROVED")], [], [], {}, ["owner"])
        self.assertEqual(found, [])

    def test_missing_thread_for_published_comment_is_an_error_not_empty_feedback(self):
        with self.assertRaises(forge.ForgeError):
            forge.feedback(pull(), [review()], [], [{**review(2), "pull_request_review_id": 1}], {}, ["owner"])

    def test_rest_pagination_failure_does_not_return_first_page_as_complete(self):
        with mock.patch.object(forge, "api", side_effect=[[{"id": number} for number in range(100)],
                                                        forge.ForgeError("second page failed")]):
            with self.assertRaises(forge.ForgeError):
                forge.pages("endpoint", time.monotonic() + 90)

    def test_rest_pagination_keeps_feedback_on_second_page(self):
        with mock.patch.object(forge, "api", side_effect=[list(range(100)), [100]]) as api:
            self.assertEqual(forge.pages("endpoint?filter=latest", time.monotonic() + 90), list(range(101)))
        self.assertIn("&per_page=100&page=2", api.call_args.args[0])

    def test_graphql_repeated_cursor_fails_before_any_partial_result(self):
        page = {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": [], "pageInfo": {"hasNextPage": True, "endCursor": "same"},
        }}}}
        with mock.patch.object(forge, "graphql", return_value=page):
            with self.assertRaises(forge.ForgeError):
                forge.thread_roots("owner/repo", 7, time.monotonic() + 90)

    def test_authentication_and_rate_limit_failures_have_distinct_retry_policy(self):
        unauthorized = forge.ForgeError("Resource not accessible", 403)
        limited = forge.ForgeError("API rate limit", 403, {
            "x-ratelimit-remaining": "0", "x-ratelimit-reset": str(int(time.time()) + 400),
        })
        secondary = forge.ForgeError("Secondary rate limit", 403, {"retry-after": "120"})
        self.assertTrue(unauthorized.permission)
        self.assertFalse(limited.permission)
        self.assertGreaterEqual(limited.retry_after, 399)
        self.assertFalse(secondary.permission)
        self.assertEqual(secondary.retry_after, 120)

    def test_nonzero_github_command_retains_http_failure_classification(self):
        raw = "HTTP/2.0 403 Forbidden\nRetry-After: 90\n\n" + json.dumps({"message": "rate limit"})
        failure = subprocess.CalledProcessError(1, ["gh"], output=raw)
        with mock.patch.object(forge, "github", side_effect=failure):
            with self.assertRaises(forge.ForgeError) as result:
                forge.api("user")
        self.assertFalse(result.exception.permission)
        self.assertEqual(result.exception.retry_after, 90)
        self.assertEqual(forge.response("HTTP/2.0 204 No Content\nDate: now\n\n")[2], {})

    def snapshot_with(self, checks, after=None):
        state, _ = task(self)
        pr = pull()
        def pages(endpoint, deadline, field=None):
            return checks if field == "check_runs" else []
        with mock.patch.object(forge, "pages", side_effect=pages), mock.patch.object(
                forge, "api", side_effect=[pr, after or copy.deepcopy(pr)]):
            return forge.snapshot(state)

    def test_zero_missing_or_pending_checks_cannot_announce_readiness(self):
        self.assertEqual(self.snapshot_with([])["ci"], "pending")
        check = {"id": 1, "name": "changes", "head_sha": HEAD, "status": "in_progress",
                 "conclusion": None, "html_url": "https://github.com/check"}
        self.assertEqual(self.snapshot_with([check])["ci"], "pending")
        check.update(status="completed", conclusion="success")
        self.assertEqual(self.snapshot_with([check])["ci"], "passed")
        check["name"] = "unrelated"
        self.assertEqual(self.snapshot_with([check])["ci"], "pending")

    def test_duplicate_check_names_cannot_hide_pending_checks(self):
        check = {"id": 1, "name": "changes", "head_sha": HEAD, "status": "in_progress",
                 "conclusion": None, "html_url": "https://github.com/check"}
        passing = {**check, "id": 2, "status": "completed", "conclusion": "success"}
        self.assertEqual(self.snapshot_with([check, passing])["ci"], "pending")

    def test_unrequired_conditional_skips_do_not_block_successful_required_checks(self):
        required = {"id": 1, "name": "changes", "head_sha": HEAD, "status": "completed",
                    "conclusion": "success", "html_url": "https://github.com/check"}
        for conclusion in ("skipped", "neutral"):
            with self.subTest(conclusion=conclusion):
                optional = {**required, "id": 2, "name": "Validate context records",
                            "conclusion": conclusion}
                self.assertEqual(self.snapshot_with([required, optional])["ci"], "passed")
                self.assertEqual(self.snapshot_with(
                    [{**required, "conclusion": conclusion}, optional])["ci"], "pending")
                self.assertEqual(self.snapshot_with([
                    required, {**required, "id": 3, "conclusion": conclusion},
                ])["ci"], "pending")

    def test_optional_checks_still_block_while_running_failed_or_unknown(self):
        required = {"id": 1, "name": "changes", "head_sha": HEAD, "status": "completed",
                    "conclusion": "success", "completed_at": "one",
                    "html_url": "https://github.com/check"}
        optional = {**required, "id": 2, "name": "optional"}
        for status, conclusion, expected in (
            ("in_progress", None, "pending"), ("completed", "failure", "failed"),
            ("completed", "cancelled", "failed"), ("completed", "unknown", "pending"),
        ):
            with self.subTest(status=status, conclusion=conclusion):
                self.assertEqual(self.snapshot_with([
                    required, {**optional, "status": status, "conclusion": conclusion},
                ])["ci"], expected)

    def test_new_check_execution_on_same_head_has_new_action_identity(self):
        check = {"id": 1, "name": "changes", "head_sha": HEAD, "status": "completed",
                 "conclusion": "failure", "completed_at": "one", "html_url": "https://github.com/check"}
        first = self.snapshot_with([check])["items"][0]["key"]
        check["completed_at"] = "two"
        second = self.snapshot_with([check])["items"][0]["key"]
        self.assertNotEqual(first, second)

    def test_pr_change_during_snapshot_discards_result(self):
        changed = pull()
        changed["head"]["sha"] = "c" * 40
        with self.assertRaises(forge.ForgeError):
            self.snapshot_with([], changed)
