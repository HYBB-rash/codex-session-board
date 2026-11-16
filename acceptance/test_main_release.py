import base64
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from scripts import main_release as release
from scripts import main_release_evidence as evidence


class MainReleaseTests(unittest.TestCase):
    def test_pending_requests_select_latest_main_and_pin_its_implementation(self):
        with patch.object(evidence, "git", side_effect=["", "newest", '[package]\nversion="0.3.98"']), \
                patch.object(evidence.subprocess, "run"), \
                patch.object(evidence, "implementation", return_value="code") as canonical, \
                patch.object(evidence, "github", return_value=json.dumps({"object": {"sha": "newest"}})):
            selected = evidence.select("owner/repo")
        canonical.assert_called_once_with("newest")
        self.assertEqual((selected["main"], selected["source"], selected["tag"]),
                         ("newest", "code", "v0.3.98"))

    def test_selection_race_or_dirty_tree_cannot_publish_a_mixed_source(self):
        with patch.object(evidence, "git", return_value=" M source"), \
                self.assertRaisesRegex(ValueError, "clean"):
            evidence.select("owner/repo")
        with patch.object(evidence, "git", side_effect=["", "older"]), \
                patch.object(evidence.subprocess, "run"), \
                patch.object(evidence, "github", return_value=json.dumps({"object": {"sha": "newer"}})), \
                self.assertRaisesRegex(ValueError, "unavailable"):
            evidence.select("owner/repo")

    def test_only_regular_context_records_share_the_previous_implementation(self):
        parents = {"record2": "record2 record1", "record1": "record1 code", "code": "code previous"}
        def changed(before, after):
            return {"source.rs"} if after == "code" else {"context/result.json"}
        with patch.object(evidence, "git", side_effect=lambda *args: parents[args[-1]]), \
                patch.object(evidence, "changed", side_effect=changed), \
                patch.object(evidence, "modes", return_value={"context/result.json": "100644"}):
            self.assertEqual(evidence.implementation("record2"), "code")
        with patch.object(evidence, "git", return_value=parents["record2"]), \
                patch.object(evidence, "changed", return_value={"context/result.json"}), \
                patch.object(evidence, "modes", return_value={"context/result.json": "120000"}):
            self.assertEqual(evidence.implementation("record2"), "record2")

    def test_main_must_have_a_successful_frozen_merge_run(self):
        runs = [{"id": 1, "head_sha": "source", "head_branch": "codex/integration-old", "event": "push"},
                {"id": 2, "head_sha": "source", "head_branch": "codex/integration-new", "event": "push"},
                {"id": 3, "head_sha": "other", "head_branch": "codex/integration-new", "event": "push"}]
        with patch.object(evidence, "github", side_effect=[
                json.dumps({"workflow_runs": runs}), json.dumps({"jobs": []})]), \
                patch.object(evidence, "validate_run") as validate:
            self.assertEqual(evidence.merge_run("owner/repo", "source")["id"], 2)
            self.assertEqual(validate.call_args.args[0]["id"], 2)
        with patch.object(evidence, "github", return_value='{"workflow_runs":[]}'), \
                self.assertRaisesRegex(ValueError, "no frozen"):
            evidence.merge_run("owner/repo", "source")

    def test_release_reuses_both_merge_reports_without_rebuilding_or_running_tests(self):
        source = {"commit": "source"}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "evidence"
            with patch.object(evidence, "identity", return_value=source), \
                    patch.object(evidence, "git", return_value=""), \
                    patch.object(evidence.subprocess, "run") as run, \
                    patch.object(evidence, "merge_run", return_value={"id": 7}), \
                    patch.object(evidence, "platform_evidence", side_effect=lambda *args: (
                        args[3], {"validation_sha256": "digest"})) as collect:
                result = evidence.verify_merge("owner/repo", output)
            self.assertEqual(set(result["platforms"]), set(evidence.PLATFORMS))
            self.assertEqual(collect.call_count, 2)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][:3], ["git", "merge-base", "--is-ancestor"])
            self.assertEqual(json.loads((output / "MERGE.json").read_text())["merge_run"], 7)

    def test_dispatch_error_keeps_merge_complete_and_release_pending(self):
        with patch.object(release, "github", side_effect=RuntimeError("transport failed")):
            with patch("scripts.main_release_feedback.failure", side_effect=RuntimeError("offline")), \
                    patch.object(release, "GitHub"):
                result = release.dispatch("owner/repo", "source")
        self.assertEqual(result["status"], "pending")
        self.assertEqual(result["requested_source"], "source")
        self.assertIn("transport failed", result["dispatch_error"])
        self.assertIn("release main dispatch", result["retry"])
        self.assertEqual(result["attempts"], 3)
        self.assertIn("scheduled", result["automatic_retry"])

    def test_channel_status_requires_both_exact_sources_and_preserves_previous_release(self):
        previous = {target: {"source_commit": "previous", "version": "0.3.97"} for target in release.TARGETS}
        forge = Mock()
        forge.api.return_value = [{"context": "Plugin release", "state": "failure"}]
        with patch.object(release, "GitHub", return_value=forge), \
                patch.object(release, "channels", return_value=previous), \
                patch.object(release, "git", return_value="main"), \
                patch.object(release, "ancestor", side_effect=[True, False]), \
                patch.object(release, "implementation", return_value="current"):
            result = release.status("owner/repo", "current")
        self.assertEqual(result["merge_status"], "merged")
        self.assertEqual(result["release_status"], "failed")
        self.assertEqual(result["published"], previous)
        with patch.object(release, "channels", return_value=previous), \
                self.assertRaisesRegex(ValueError, "Both delivery channels"):
            release.record("owner/repo", "current", "success", 7)

    def test_published_descendant_covers_coalesced_and_older_main_commits(self):
        current = {target: {"source_commit": "newer", "version": "0.3.98"} for target in release.TARGETS}
        forge = Mock()
        forge.api.return_value = [{"context": "Plugin release", "state": "pending"}]
        with patch.object(release, "GitHub", return_value=forge), \
                patch.object(release, "channels", return_value=current), \
                patch.object(release, "implementation", return_value="older"), \
                patch.object(release, "git", return_value="main"), \
                patch.object(release, "ancestor", return_value=True):
            result = release.status("owner/repo", "older")
        self.assertEqual(result["release_status"], "covered")
        self.assertEqual(result["covered_by"], "newer")

    def test_missing_immutable_release_or_wrong_tag_cannot_skip_publication(self):
        selected = {"source": "source", "version": "0.3.98", "tag": "v0.3.98"}
        manifests = {target: {"source_commit": "source", "version": "0.3.98", "target": target}
                     for target in release.TARGETS}
        forge = Mock()
        with patch.object(release, "GitHub", return_value=forge), \
                patch.object(release, "channels", return_value=manifests):
            for saved in (None, {"draft": True, "immutable": True},
                          {"draft": False, "immutable": False}):
                forge.release.return_value = saved
                self.assertFalse(release.already_published("owner/repo", selected))
            forge.release.return_value = {"draft": False, "immutable": True}
            forge.api.return_value = {"sha": "other"}
            self.assertFalse(release.already_published("owner/repo", selected))
            forge.api.return_value = {"sha": "source"}
            self.assertTrue(release.already_published("owner/repo", selected))

    def test_changed_implementation_needs_a_fresh_unclaimed_version(self):
        with patch.object(evidence, "git", side_effect=['[package]\nversion="0.3.97"'] * 2), \
                self.assertRaisesRegex(ValueError, "fresh"):
            evidence.require_version("owner/repo", "base")
        forge = Mock()
        forge.api.side_effect = [{}, {"sha": "other"}]
        with patch("scripts.release_plugin.GitHub", return_value=forge), \
                patch.object(evidence, "git", side_effect=[
                    '[package]\nversion="0.3.97"', '[package]\nversion="0.3.98"', "source"]), \
                self.assertRaisesRegex(ValueError, "different immutable source"):
            evidence.require_version("owner/repo", "base")

    def test_old_success_status_without_delivery_evidence_cannot_claim_publication(self):
        forge = Mock()
        forge.api.return_value = [{"context": "Plugin release", "state": "success"}]
        with patch.object(release, "GitHub", return_value=forge), \
                patch.object(release, "channels", return_value={target: None for target in release.TARGETS}), \
                patch.object(release, "git", return_value="main"), \
                patch.object(release, "implementation", return_value="source"), \
                patch.object(release, "ancestor", return_value=True):
            result = release.status("owner/repo", "source")
        self.assertEqual(result["release_status"], "inconsistent")
        self.assertIsNone(result["covered_by"])
        self.assertEqual(result["latest_attempt"]["state"], "success")
    def test_channel_manifest_reads_existing_candidate_branches(self):
        forge = Mock()
        forge.api.return_value = {"content": base64.b64encode(b'{"source_commit":"fixed"}').decode()}
        self.assertEqual(len(release.channels(forge)), 2)
        self.assertTrue(all("candidate%2F" in call.args[0] for call in forge.api.call_args_list))
