import hashlib
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import release_plugin


class SavedGitHub:
    def __init__(self):
        self.repository = "owner/plugin"
        self.assets = {}
        self.draft = True
        self.source = "a" * 40
        self.uploaded = []
        self.edits = []

    def release(self, tag):
        return {"draft": self.draft, "immutable": not self.draft, "assets": [
            {"name": name, "digest": "sha256:" + hashlib.sha256(body).hexdigest()}
            for name, body in self.assets.items()
        ]}

    def upload(self, tag, path):
        self.uploaded.append(path.name)
        self.assets.setdefault(path.name, path.read_bytes())

    def download(self, tag, name, directory):
        result = directory / name
        result.write_bytes(self.assets[name])
        return result

    def api(self, path):
        return {"sha": self.source} if path.startswith("commits/") else {"enabled": True}

    def gh(self, *arguments, **options):
        self.edits.append(arguments)
        if "--draft=false" in arguments:
            self.draft = False
        return SimpleNamespace(returncode=0)


class SavedReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.github = SavedGitHub()
        self.tag = "v0.2.93"
        self.target = "x86_64-unknown-linux-gnu"

    def archive(self, target=None, source=None):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            (package / "bin").mkdir()
            (package / ".codex-plugin").mkdir()
            for name in ("bin/codex-session-board", "bin/run-plugin"):
                (package / name).write_text("executable")
                (package / name).chmod(0o755)
            for name in ("plugin.json", ".codex-plugin/plugin.json"):
                (package / name).write_text('{"version":"0.2.93"}')
            files = package_plugin.hashes(package)
            (package / "RELEASE.json").write_text(json.dumps({
                "schema_version": 1, "target": target or self.target, "version": "0.2.93",
                "source_commit": source or self.github.source, "files": files,
                "binary_sha256": files["bin/codex-session-board"],
            }))
            output = io.BytesIO()
            with tarfile.open(fileobj=output, mode="w:gz") as contents:
                for path in package.iterdir():
                    contents.add(path, arcname=path.name)
            return output.getvalue()

    def save_existing(self, target=None):
        target = target or self.target
        name = release_plugin.asset_name(target)
        body = self.archive(target)
        self.github.assets[name] = body
        self.github.assets[name + ".sha256"] = f"{hashlib.sha256(body).hexdigest()}  {name}\n".encode()

    def test_local_then_ci_reuses_exact_archive_without_building(self):
        self.save_existing()
        original = dict(self.github.assets)
        for attempt in range(2):
            directory = self.root / str(attempt)
            directory.mkdir()
            with patch.object(release_plugin, "build") as build, patch.object(release_plugin, "authorize"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, directory)
                build.assert_not_called()
        self.assertEqual(self.github.assets, original)

    def test_missing_platform_stays_draft_without_channel_writes(self):
        self.save_existing()
        result = release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
        self.assertEqual(result["status"], "incomplete")
        self.assertTrue(self.github.draft)
        self.assertEqual(self.github.edits, [])

    def test_actions_token_checks_immutability_before_channel_writes(self):
        for target in package_plugin.TARGETS:
            self.save_existing(target)
        def api(path):
            if path == "immutable-releases":
                raise RuntimeError("Resource not accessible by integration (HTTP 403)")
            return {"sha": self.github.source}
        def checkout(source, destination):
            (destination / "scripts").mkdir(parents=True)
        with patch.object(self.github, "api", side_effect=api), \
                patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin.subprocess, "run") as run:
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
        self.assertFalse(self.github.draft)
        self.assertEqual(run.call_count, 4)
        observed_release = self.github.release
        with patch.object(self.github, "release", side_effect=lambda tag: {**observed_release(tag), "immutable": False}), \
                patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin.subprocess, "run") as run:
            retry = self.root / "mutable"
            retry.mkdir()
            with self.assertRaisesRegex(ValueError, "must be immutable"):
                release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, retry)
            self.assertEqual(run.call_count, 2)

    def test_mismatched_source_is_rejected_before_build_or_upload(self):
        self.github.assets[release_plugin.asset_name(self.target)] = self.archive(source="b" * 40)
        with patch.object(release_plugin, "build") as build:
            with self.assertRaisesRegex(ValueError, "different source"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, self.root)
            build.assert_not_called()
        self.assertEqual(self.github.uploaded, [])

    def test_upload_race_consumes_winner_without_replacing_it(self):
        winner = self.archive()

        def build(root, target, destination):
            archive = self.root / "fixture.tar.gz"
            archive.write_bytes(winner)
            release_plugin.unpack(archive, destination)
            self.github.assets[release_plugin.asset_name(target)] = winner
            return destination

        with patch.object(release_plugin, "build", side_effect=build), patch.object(release_plugin, "authorize"):
            release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, self.root)
        self.assertEqual(self.github.assets[release_plugin.asset_name(self.target)], winner)

    def test_publish_retry_and_stable_keep_original_bytes(self):
        for target in release_plugin.TARGETS:
            self.save_existing(target)
        original = dict(self.github.assets)
        evidence = self.root / "evidence.json"
        evidence.write_text(json.dumps({"platforms": {target: "review.json" for target in release_plugin.TARGETS}}))
        (self.root / "review.json").write_text("{}")

        def checkout(source, directory):
            (directory / "scripts").mkdir(parents=True)

        with patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin.subprocess, "run") as run:
            run.side_effect = [SimpleNamespace(returncode=0), SimpleNamespace(returncode=0), RuntimeError("push interrupted")]
            with self.assertRaisesRegex(RuntimeError, "interrupted"):
                release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
            self.assertFalse(self.github.draft)
            for channel in ("candidate", "stable"):
                directory = self.root / channel
                directory.mkdir()
                run.side_effect = None
                release_plugin.publish_saved(self.github, self.tag, self.github.source, channel,
                                             evidence if channel == "stable" else None, directory)
        self.assertEqual(self.github.assets, original)
        self.assertEqual(self.github.uploaded, [])

    def test_stable_requires_evidence_and_complete_packages_before_writes(self):
        with self.assertRaisesRegex(ValueError, "reviewed evidence"):
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "stable", None, self.root)
        evidence = self.root / "evidence.json"
        evidence.write_text("{}")
        with self.assertRaisesRegex(ValueError, "missing required"):
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "stable", evidence, self.root)
        self.assertEqual(self.github.edits, [])

    def test_archive_links_and_checksum_tampering_are_rejected(self):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w:gz") as contents:
            entry = tarfile.TarInfo("link")
            entry.type = tarfile.SYMTYPE
            entry.linkname = "/etc/passwd"
            contents.addfile(entry)
        archive = self.root / "unsafe.tar.gz"
        archive.write_bytes(output.getvalue())
        with self.assertRaisesRegex(ValueError, "links"):
            release_plugin.unpack(archive, self.root / "unsafe")
        self.save_existing()
        self.github.assets[release_plugin.asset_name(self.target) + ".sha256"] = b"wrong"
        with patch.object(release_plugin, "authorize"):
            with self.assertRaisesRegex(ValueError, "checksum"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, self.root)


if __name__ == "__main__":
    unittest.main()
