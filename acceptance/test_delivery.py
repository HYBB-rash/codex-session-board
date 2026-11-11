import json
import hashlib
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import publish_plugin


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.package = self.root / "package"
        (self.package / "bin").mkdir(parents=True)
        (self.package / ".codex-plugin").mkdir()
        (self.package / "bin/codex-session-board").write_bytes(b"candidate executable")
        (self.package / "bin/run-plugin").write_text("launcher")
        for name in ("codex-session-board", "run-plugin"):
            (self.package / "bin" / name).chmod(0o755)
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            (self.package / name).write_text(json.dumps({"version": "0.2.93"}))
        self.manifest = {
            "schema_version": 1, "target": "x86_64-unknown-linux-gnu", "version": "0.2.93",
            "source_commit": "a" * 40, "build_sha256": "b" * 64,
            "files": package_plugin.hashes(self.package),
        }
        self.manifest["binary_sha256"] = self.manifest["files"]["bin/codex-session-board"]
        self.manifest["contract_sha256"] = hashlib.sha256(b"contract").hexdigest()
        self.write_manifest()
        (self.root / "acceptance").mkdir()
        (self.root / "acceptance/bindings.json").write_text('{"fingerprint_paths":["source"]}')
        (self.root / "acceptance/requirements.json").write_bytes(b"contract")
        (self.root / "Cargo.toml").write_text('[package]\nname="codex-session-board"\nversion="0.2.93"\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname="codex-session-board"\nversion="0.2.93"\n')
        (self.root / ".codex-plugin").mkdir()
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            (self.root / name).write_text((self.package / name).read_text())

    def write_manifest(self):
        (self.package / "RELEASE.json").write_text(json.dumps(self.manifest))

    def test_package_hashes_bind_assets_binary_and_version(self):
        self.assertEqual(package_plugin.verify_package(self.package), self.manifest)
        (self.package / "bin/codex-session-board").write_bytes(b"other executable")
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)

    def test_extra_files_and_symlinks_cannot_enter_a_release(self):
        (self.package / "unexpected").write_text("runtime data")
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)
        (self.package / "unexpected").unlink()
        (self.package / "alias").symlink_to(self.package / "plugin.json")
        with self.assertRaisesRegex(ValueError, "symlinks"):
            package_plugin.verify_package(self.package)

    def test_native_cache_metadata_is_excluded_only_for_installed_packages(self):
        metadata = self.package / ".codex-marketplace-install.json"
        metadata.write_text('{"source_type":"git"}')
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)
        self.assertEqual(package_plugin.verify_package(self.package, installed=True), self.manifest)
        (self.package / "bin/run-plugin").chmod(0o644)
        with self.assertRaisesRegex(ValueError, "executable"):
            package_plugin.verify_package(self.package, installed=True)

    def test_source_fingerprint_changes_reject_publication(self):
        with patch.object(publish_plugin, "command", return_value="a" * 40):
            with patch.object(publish_plugin, "fingerprint", return_value="c" * 64):
                with self.assertRaisesRegex(ValueError, "stale"):
                    publish_plugin.authorize(self.package, "candidate", None, self.root)

    def test_stable_requires_full_gate_even_for_valid_candidate(self):
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with self.assertRaisesRegex(ValueError, "reviewed full"):
                    publish_plugin.authorize(self.package, "stable", None, self.root)
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with patch.object(publish_plugin, "verify", return_value=({"release_ready": False}, 2)):
                    with self.assertRaisesRegex(ValueError, "has not passed"):
                        publish_plugin.authorize(self.package, "stable", self.root / "evidence.json", self.root)
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with patch.object(publish_plugin, "verify", return_value=({"release_ready": True}, 0)):
                    self.assertEqual(
                        publish_plugin.authorize(self.package, "stable", self.root / "evidence.json", self.root),
                        self.manifest)
