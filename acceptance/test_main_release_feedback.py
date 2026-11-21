import unittest
from unittest.mock import Mock, patch

from scripts import main_release_feedback as feedback


class MainReleaseFeedbackTests(unittest.TestCase):
    def test_failure_reuses_one_source_bound_incident_and_reports_previous_channels(self):
        source = "a" * 40
        incident = {"number": 19, "body": f"<!-- main-release:failure {source} -->",
                    "html_url": "issue-url"}
        forge = Mock()
        forge.pages.return_value = [incident]
        forge.api.return_value = incident
        with patch.object(feedback, "GitHub", return_value=forge), \
                patch.object(feedback, "channels", return_value={"target": {"source_commit": "old"}}), \
                patch.object(feedback, "ensure_label"):
            result = feedback.failure("owner/repo", source, 8, "run-url")
        forge.api.assert_called_once_with("issues/19")
        self.assertEqual(result["issue"], "issue-url")
        self.assertEqual(result["previous"]["target"]["source_commit"], "old")
        self.assertEqual(result["source"], source)

    def test_recovery_closes_only_incidents_covered_by_the_published_source(self):
        source, unrelated = "a" * 40, "b" * 40
        issues = [{"number": number, "body": f"<!-- main-release:failure {head} -->"}
                  for number, head in ((19, source), (20, unrelated))]
        def ancestor(*arguments):
            if arguments[-2] == unrelated:
                raise RuntimeError("unrelated branch")
        forge = Mock()
        forge.pages.return_value = issues
        with patch.object(feedback, "GitHub", return_value=forge), \
                patch.object(feedback, "git", side_effect=ancestor), \
                patch.object(feedback, "comment_once") as comment:
            self.assertEqual(feedback.recovered("owner/repo", source, 8), [19])
        self.assertEqual(comment.call_count, 1)
        self.assertEqual(forge.api.call_count, 2)

    def test_closed_unacknowledged_gate_gap_needs_a_regression_for_recovery(self):
        source = "a" * 40
        issue = {"number": 19, "state": "closed", "body": f"<!-- main-release:failure {source} -->",
                 "labels": [{"name": "release-blocker"}, {"name": "merge-gate-gap"}]}
        forge = Mock()
        forge.pages.return_value = [issue]
        with patch.object(feedback, "GitHub", return_value=forge), \
                patch.object(feedback, "git", side_effect=["", "Unrelated change", "src/main.rs"]), \
                patch.object(feedback, "comment_once"):
            self.assertEqual(feedback.recovered("owner/repo", "newer", 8), [])
            forge.api.assert_not_called()
        with patch.object(feedback, "GitHub", return_value=forge), \
                patch.object(feedback, "git", side_effect=["", "Release-Fix: #19", "acceptance/test_release.py"]), \
                patch.object(feedback, "comment_once"):
            self.assertEqual(feedback.recovered("owner/repo", "newer", 8), [19])
        issue["labels"].append({"name": "release-published"})
        with patch.object(feedback, "GitHub", return_value=forge), patch.object(feedback, "git") as git:
            self.assertEqual(feedback.recovered("owner/repo", "newer", 8), [])
            git.assert_not_called()

    def test_recovery_comment_is_reused_and_confirmed_by_id(self):
        forge = Mock()
        saved = {"id": 2, "body": "<!-- marker -->\nrecovered", "html_url": "comment-url"}
        forge.pages.return_value = [saved]
        forge.api.return_value = saved
        self.assertEqual(feedback.comment_once(forge, 19, "<!-- marker -->", "recovered"), "comment-url")
        forge.api.assert_called_once_with("issues/comments/2")
        with self.assertRaisesRegex(ValueError, "did not confirm"):
            feedback.comment_once(forge, 19, "<!-- marker -->", "different source")
