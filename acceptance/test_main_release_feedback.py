import unittest
from unittest.mock import Mock, patch

from scripts import main_release_feedback as feedback


class MainReleaseFeedbackTests(unittest.TestCase):
    def test_broken_main_blocks_unrelated_merges_until_a_referenced_regression_fix(self):
        incident = {"number": 19, "body": "<!-- main-release:failure " + "a" * 40 + " -->",
                    "labels": [{"name": "merge-gate-gap"}]}
        with patch.object(feedback.forge, "pages", return_value=[incident]):
            for messages, paths in (("Update UI", "tests/ui_layout.rs"),
                                    ("Release-Fix: #19", "src/main.rs")):
                with patch.object(feedback, "git", side_effect=[messages, paths]), \
                        self.assertRaisesRegex(ValueError, "unresolved release incident #19"):
                    feedback.require_recovery("owner/repo", "base")
            with patch.object(feedback, "git", side_effect=["Release-Fix: #19", "acceptance/test_release.py"]):
                feedback.require_recovery("owner/repo", "base")

    def test_infrastructure_recovery_does_not_require_an_unrelated_business_regression(self):
        incident = {"number": 19, "body": "<!-- main-release:failure " + "a" * 40 + " -->", "labels": []}
        with patch.object(feedback.forge, "pages", return_value=[incident]), \
                patch.object(feedback, "git", side_effect=["Release-Fix: #19", ".github/workflows/release.yml"]):
            feedback.require_recovery("owner/repo", "base")

    def test_failure_reuses_one_incident_and_confirms_source_bound_author_feedback(self):
        source = "a" * 40
        incident = {"number": 19, "body": f"<!-- main-release:failure {source} -->",
                    "html_url": "issue-url"}
        pr = {"number": 7, "user": {"login": "author"}}
        with patch.object(feedback, "channels", return_value={"target": {"source_commit": "old"}}), \
                patch.object(feedback, "ensure_label"), \
                patch.object(feedback.forge, "pages", return_value=[incident]), \
                patch.object(feedback.forge, "api", return_value=incident) as api, \
                patch.object(feedback, "affected_prs", return_value={7: pr}), \
                patch.object(feedback, "comment_once", return_value="comment-url") as comment:
            result = feedback.failure("owner/repo", source, 8, "run-url")
        self.assertEqual(api.call_count, 1)
        self.assertEqual(result["issue"], "issue-url")
        self.assertEqual(result["previous"]["target"]["source_commit"], "old")
        self.assertFalse(result["authors"][0]["accepted"])
        self.assertIn(source, comment.call_args.args[3])
        self.assertIn("author", comment.call_args.args[3])

    def test_recovery_closes_only_incidents_covered_by_the_published_source(self):
        source, unrelated = "a" * 40, "b" * 40
        issues = [{"number": number, "body": f"<!-- main-release:failure {head} -->"}
                  for number, head in ((19, source), (20, unrelated))]
        def ancestor(*arguments):
            if arguments[-2] == unrelated:
                raise RuntimeError("unrelated branch")
        with patch.object(feedback.forge, "pages", return_value=issues), \
                patch.object(feedback, "git", side_effect=ancestor), \
                patch.object(feedback, "comment_once") as comment, \
                patch.object(feedback.forge, "api") as api:
            self.assertEqual(feedback.recovered("owner/repo", source, 8), [19])
        self.assertEqual(comment.call_count, 1)
        self.assertEqual(api.call_count, 2)

    def test_closed_unacknowledged_incident_still_blocks_and_gate_gap_needs_a_regression(self):
        source = "a" * 40
        issue = {"number": 19, "state": "closed", "body": f"<!-- main-release:failure {source} -->",
                 "labels": [{"name": "release-blocker"}, {"name": "merge-gate-gap"}]}
        with patch.object(feedback.forge, "pages", return_value=[issue]), \
                patch.object(feedback, "git", side_effect=["", "Unrelated change", "src/main.rs"]), \
                patch.object(feedback.forge, "api") as api, \
                patch.object(feedback, "comment_once"):
            self.assertEqual(feedback.recovered("owner/repo", "newer", 8), [])
            api.assert_not_called()
        issue["labels"].append({"name": "release-published"})
        with patch.object(feedback.forge, "pages", return_value=[issue]), \
                patch.object(feedback, "git") as git:
            feedback.require_recovery("owner/repo", "base")
            git.assert_not_called()
