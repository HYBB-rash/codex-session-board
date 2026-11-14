import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import verify_plugin


class VerifyPluginTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.package = self.root / "package"
        (self.package / "bin").mkdir(parents=True)
        for name in ("bin/codex-session-board", "bin/run-plugin"):
            path = self.package / name
            path.write_bytes(b"verified executable")
            path.chmod(0o755)
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            path = self.package / name
            path.parent.mkdir(exist_ok=True)
            path.write_text('{"version":"0.3.75"}')
        files = package_plugin.hashes(self.package)
        self.manifest = {
            "schema_version": 1, "version": "0.3.75", "target": "aarch64-apple-darwin",
            "source_commit": "a" * 40, "files": files,
            "binary_sha256": files["bin/codex-session-board"],
        }
        (self.package / "RELEASE.json").write_text(json.dumps(self.manifest))
        self.state = self.root / "installation"
        self.state.mkdir()
        (self.state / "current.json").write_text(json.dumps({
            "installed": True, "desktop_loaded": False, "release": self.manifest,
            "package": str(self.package), "revision": "b" * 40,
        }))
        self.config = self.root / "config.json"
        self.config.write_text(json.dumps({"port": 18767, "data_dir": str(self.root / "data")}))
        self.info = {
            "app": "codex-session-board", "version": "0.3.75", "pid": 200,
            "build": self.manifest["binary_sha256"], "configId": "current-config",
            "dataDir": str(self.root / "data"),
        }
        self.runtime = {"pid": 200, "parent_pid": 100, "started": "now",
                        "command": "board mcp",
                        "executable": str(self.package / "bin/codex-session-board")}
        self.client = {"pid": 100, "parent_pid": 1, "started": "earlier",
                       "command": "codex app-server", "executable": "/client"}

    def inspect(self):
        with patch.object(verify_plugin, "cache_directory", return_value=self.package), \
                patch.object(verify_plugin, "runtime_info", return_value=self.info), \
                patch.object(verify_plugin, "process_identity",
                             side_effect=lambda pid: self.runtime if pid == 200 else self.client):
            return verify_plugin.inspect(self.state, self.config, 100, "a" * 40, "current-config")

    def test_current_client_is_confirmed_from_live_identity_not_old_receipt(self):
        result = self.inspect()
        self.assertTrue(result["passed"])
        self.assertTrue(result["desktop_loaded"])
        self.assertTrue(result["expected_config_id_checked"])
        self.assertEqual(result["config_sha256"], hashlib.sha256(self.config.read_bytes()).hexdigest())

    def test_old_runtime_remains_waiting_despite_successful_install(self):
        self.info["version"] = "old"
        result = self.inspect()
        self.assertFalse(result["passed"])
        self.assertEqual(result["phase"], "awaiting_desktop_reload")

    def test_unrelated_runtime_cannot_sign_for_the_selected_client(self):
        self.runtime["parent_pid"] = 999
        self.assertFalse(self.inspect()["desktop_loaded"])

    def test_tampered_cache_is_rejected_before_runtime_probe(self):
        (self.package / "bin/codex-session-board").write_bytes(b"tampered")
        with patch.object(verify_plugin, "runtime_info") as probe:
            with self.assertRaisesRegex(ValueError, "hashes differ"):
                self.inspect()
        probe.assert_not_called()

    def test_wrong_data_or_configuration_identity_is_rejected(self):
        for key, value in (("dataDir", "/another-board"), ("configId", "old-config")):
            with self.subTest(key=key):
                original = self.info[key]
                self.info[key] = value
                with self.assertRaises(ValueError):
                    self.inspect()
                self.info[key] = original

    def test_pid_reuse_during_inspection_is_rejected(self):
        identities = [self.runtime, self.client, {**self.runtime, "started": "later"}]
        with patch.object(verify_plugin, "cache_directory", return_value=self.package), \
                patch.object(verify_plugin, "runtime_info", return_value=self.info), \
                patch.object(verify_plugin, "process_identity", side_effect=identities):
            with self.assertRaisesRegex(ValueError, "changed during"):
                verify_plugin.inspect(self.state, self.config, 100)
