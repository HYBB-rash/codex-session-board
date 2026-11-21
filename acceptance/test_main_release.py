import base64
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib.parse import parse_qs, urlsplit

from scripts import main_release as release
from scripts import main_release_evidence as evidence
from scripts import validate_candidate
from acceptance.test_gate import GateFixture


class ReleaseCandidateEvidenceTests(GateFixture):
    def build_local_candidate(self, output):
        scripts = self.root / "scripts"
        scripts.mkdir()
        shutil.copy(release.ROOT / "scripts/verify_acceptance.py", scripts)
        (scripts / "check").write_text("#!/bin/sh\ntest -s source.txt\n")
        (scripts / "check").chmod(0o700)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname="release-fixture"\nversion="0.1.0"\nedition="2021"\n')
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").touch()
        subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=self.root,
                       check=True, capture_output=True, timeout=15)
        source = {"commit": "fixture-commit", "tree": "fixture-tree",
                  "fingerprint": validate_candidate.fingerprint(self.root, self.bindings["fingerprint_paths"]),
                  "platform": sys.platform, "machine": platform.machine()}
        with patch.object(validate_candidate, "ROOT", self.root.resolve()), \
                patch.object(validate_candidate, "identity", return_value=source), \
                patch.object(validate_candidate, "git", return_value=""), \
                patch.dict(os.environ, {"CARGO_TARGET_DIR": str(output.parent / "target")}), \
                patch.object(sys, "argv", ["validate", "--output", str(output)]):
            self.assertEqual(validate_candidate.main(), 0)
        return source

    def test_publication_accepts_only_complete_matching_candidate_evidence(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        artifact = Path(directory.name) / "candidate"
        source = self.build_local_candidate(artifact)
        record = json.loads((artifact / "VALIDATION.json").read_text())
        native = (sys.platform, platform.machine())
        label = next((key for key, target in evidence.PLATFORMS.items() if target == native), "fixture-native")
        other = next(key for key in evidence.PLATFORMS if key != label)

        def collect(platform_label=label, pinned=source, replacement=None):
            def download(*arguments):
                folder = Path(arguments[-1])
                shutil.copytree(artifact, folder, dirs_exist_ok=True)
                if replacement:
                    name, content = replacement
                    (folder / name).write_text(content)
            with tempfile.TemporaryDirectory(dir=directory.name) as destination:
                return evidence.platform_evidence(Mock(gh=download), 7, pinned, platform_label, 1, Path(destination))

        with patch.dict(evidence.PLATFORMS, {label: native}):
            received, report = collect()
            self.assertEqual(received, label)
            self.assertTrue(report["report"]["scope_passed"])
            with self.assertRaisesRegex(ValueError, "pinned release source"):
                collect(platform_label=other)
            for field in ("commit", "fingerprint"):
                with self.subTest(field=field), self.assertRaisesRegex(ValueError, "pinned release source"):
                    collect(pinned={**source, field: "other-fixture"})
            for scenario, file, content, message in (
                ("automatic report changed", "automated.json", "{}", "Automatic acceptance evidence changed"),
                ("step output changed", "step-1.log", "other output", "步骤日志或进程清理凭据"),
                ("required step missing", "VALIDATION.json", json.dumps({**record, "steps": record["steps"][:-1]}),
                 "步骤、执行计划或清理集合不完整"),
                ("receipts belong to a different run", "VALIDATION.json",
                 json.dumps({**record, "run_id": "different-fixture-run"}), "步骤日志或进程清理凭据"),
            ):
                with self.subTest(scenario=scenario), self.assertRaisesRegex(ValueError, message):
                    collect(replacement=(file, content))


class MainReleaseTests(unittest.TestCase):
    def test_pending_requests_select_the_latest_frozen_main_commit(self):
        main, source = "b" * 40, "a" * 40
        with patch.object(evidence, "git", side_effect=["", main, source, '[package]\nversion="0.3.98"']) as git, \
                patch.object(evidence.subprocess, "run") as run, \
                patch.object(evidence, "GitHub") as forge:
            forge.return_value.api.return_value = {"object": {"sha": main}}
            selected = evidence.select("owner/repo")
        self.assertIn("-G^version[[:space:]]*=", git.call_args_list[2].args)
        self.assertEqual(run.call_args.args[0], ["git", "merge-base", "--is-ancestor", source, main])
        self.assertEqual((selected["main"], selected["source"], selected["tag"]),
                         (main, source, "v0.3.98"))

    def test_selection_race_or_dirty_tree_cannot_publish_a_mixed_source(self):
        with patch.object(evidence, "git", return_value=" M source"), \
                self.assertRaisesRegex(ValueError, "clean"):
            evidence.select("owner/repo")
        with patch.object(evidence, "git", side_effect=["", "older"]), \
                patch.object(evidence.subprocess, "run"), \
                patch.object(evidence, "GitHub") as forge, \
                self.assertRaisesRegex(ValueError, "unavailable"):
            forge.return_value.api.return_value = {"object": {"sha": "newer"}}
            evidence.select("owner/repo")

    def test_only_regular_context_records_share_the_previous_implementation(self):
        parents = {"record2": "record2 record1", "record1": "record1 code", "code": "code previous"}
        def git(action, *arguments):
            if action == "rev-list":
                return parents[arguments[-1]]
            if action == "diff":
                return "source.rs\0" if arguments[-1] == "code" else "context/result.json\0"
            return "100644 blob hash\tcontext/result.json\0"
        with patch.object(evidence, "git", side_effect=git):
            self.assertEqual(evidence.implementation("record2"), "code")
        with patch.object(evidence, "git", side_effect=[
            parents["record2"], "context/result.json\0", "120000 blob hash\tcontext/result.json\0",
        ]):
            self.assertEqual(evidence.implementation("record2"), "record2")

    def test_publication_requires_both_platform_jobs_from_its_own_run(self):
        forge = Mock(repository="owner/repo")
        run = {"path": ".github/workflows/plugin-release.yml", "head_sha": "workflow",
               "repository": {"full_name": "owner/repo"}}
        jobs = [{"name": f"Validate release ({label})", "status": "completed", "conclusion": "success",
                 "head_sha": "workflow", "run_attempt": attempt}
                for attempt, label in enumerate(evidence.PLATFORMS, 1)]
        forge.api.side_effect = [run, {"jobs": jobs}]
        self.assertEqual(evidence.validation_attempts(forge, 7),
                         {label: attempt for attempt, label in enumerate(evidence.PLATFORMS, 1)})
        for changed in ([], [jobs[0]], [jobs[0], {**jobs[1], "conclusion": "failure"}],
                        [jobs[0], {**jobs[1], "head_sha": "other"}]):
            forge.api.side_effect = [run, {"jobs": changed}]
            with self.assertRaisesRegex(ValueError, "did not succeed"):
                evidence.validation_attempts(forge, 7)
        forge.api.side_effect = [{**run, "path": ".github/workflows/other.yml"}, {"jobs": jobs}]
        with self.assertRaisesRegex(ValueError, "publication workflow"):
            evidence.validation_attempts(forge, 7)

    def test_release_records_both_platform_reports_for_the_pinned_source(self):
        source = {"commit": "source"}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "evidence"
            checkout = Path(temporary) / "source"
            checkout.mkdir()
            with self.assertRaisesRegex(ValueError, "outside both checkouts"):
                evidence.verify_release("owner/repo", checkout / "evidence", checkout, "source", 7)
            with patch.object(evidence, "identity", return_value=source), \
                    patch.object(evidence, "git", return_value=""), \
                    patch.object(evidence.subprocess, "run") as run, \
                    patch.object(evidence, "validation_attempts",
                                 return_value=dict.fromkeys(evidence.PLATFORMS, 1)), \
                    patch.object(evidence, "platform_evidence", side_effect=lambda *args: (
                        args[3], {"validation_sha256": "digest"})) as collect:
                result = evidence.verify_release("owner/repo", output, checkout, "source", 7)
            self.assertEqual(set(result["platforms"]), set(evidence.PLATFORMS))
            self.assertEqual(collect.call_count, 2)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][:3], ["git", "merge-base", "--is-ancestor"])
            self.assertEqual(json.loads((output / "VALIDATED-RELEASE.json").read_text())["run"], 7)

    def test_dispatch_error_keeps_merge_complete_and_release_pending(self):
        with patch("scripts.main_release_feedback.failure", side_effect=RuntimeError("offline")), \
                patch.object(release, "GitHub") as forge:
            forge.return_value.gh.side_effect = RuntimeError("transport failed")
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

    def test_explicit_validation_pins_only_a_full_commit_contained_in_main(self):
        main, source = "b" * 40, "a" * 40
        with patch.object(evidence, "git", side_effect=["", main, '[package]\nversion="0.3.97"']) as git, \
                patch.object(evidence.subprocess, "run") as run, patch.object(evidence, "GitHub") as forge:
            forge.return_value.api.return_value = {"object": {"sha": main}}
            self.assertEqual(evidence.select("owner/repo", source)["source"], source)
            self.assertEqual(run.call_args.args[0], ["git", "merge-base", "--is-ancestor", source, main])
            self.assertEqual(git.call_count, 3)
        with patch.object(evidence, "git", side_effect=["", main]), patch.object(evidence.subprocess, "run"), \
                patch.object(evidence, "GitHub") as forge, self.assertRaisesRegex(ValueError, "full main commit"):
            forge.return_value.api.return_value = {"object": {"sha": main}}
            evidence.select("owner/repo", "main")

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
        expected = {target: {"source_commit": str(index)} for index, target in enumerate(release.TARGETS)}

        def read_manifest(endpoint):
            url = urlsplit(endpoint)
            self.assertEqual(url.path, "contents/RELEASE.json")
            [reference] = parse_qs(url.query)["ref"]
            channel, target = reference.split("/", 1)
            self.assertEqual(channel, "candidate")
            return {"content": base64.b64encode(json.dumps(expected[target]).encode()).decode()}

        forge = Mock()
        forge.api.side_effect = read_manifest
        self.assertEqual(release.channels(forge), expected)
