import contextlib
import io
import json
import os
import socket
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import reload_plugins
import update_plugin


class PluginUpdateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.state = self.root / "installation"
        self.source = self.root / "source"
        self.source.mkdir()
        self.market = self.root / "market"
        self.revision = "a" * 40
        self.version = "1.2.3"
        self.target = "x86_64-unknown-linux-gnu"
        for name, content in {
            "bin/codex-session-board": "mock executable",
            "bin/run-plugin": "mock launcher",
            "plugin.json": json.dumps({"version": self.version}),
            ".codex-plugin/plugin.json": json.dumps({"version": self.version}),
            ".agents/plugins/marketplace.json": json.dumps({"name": update_plugin.MARKET}),
        }.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
            if name.startswith("bin/"):
                path.chmod(0o755)
        files = package_plugin.hashes(self.source)
        self.manifest = {"schema_version": 1, "target": self.target,
                         "version": self.version, "source_commit": "approved-source",
                         "files": files, "binary_sha256": files["bin/codex-session-board"]}
        (self.source / "RELEASE.json").write_text(json.dumps(self.manifest))
        self.commands = []
        self.pending_at_install = None
        self.installed_revision = self.revision
        self.environment = patch.dict(os.environ, {"CODEX_HOME": str(self.home),
            "CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET": str(self.root / "absent.sock")})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def run_command(self, arguments, **kwargs):
        self.commands.append(arguments)
        if arguments[:2] == ["git", "checkout"]:
            import shutil
            shutil.copytree(self.source, kwargs["cwd"], dirs_exist_ok=True)
        if arguments[:3] == ["codex", "plugin", "add"]:
            import shutil
            self.pending_at_install = json.loads((self.state / "pending.json").read_text())
            cache = self.home / "plugins/cache" / update_plugin.MARKET / "codex-session-board" / self.version
            shutil.copytree(self.source, cache)
            (cache / ".codex-marketplace-install.json").write_text(json.dumps({
                "source_type": "git", "revision": self.installed_revision}))
            shutil.copytree(self.source, self.market)
        return SimpleNamespace(returncode=0)

    def command(self, arguments, root=None):
        self.commands.append(arguments)
        if arguments[:4] == ["codex", "plugin", "marketplace", "list"]:
            return json.dumps({"marketplaces": [{"name": update_plugin.MARKET,
                "root": str(self.market), "marketplaceSource": {"sourceType": "git"}}]})
        if arguments == ["git", "rev-parse", "HEAD"]:
            return self.revision
        if arguments[-1] == "--version":
            return f"codex-session-board {self.version}"
        raise AssertionError(arguments)

    @contextlib.contextmanager
    def isolated(self):
        with patch.object(update_plugin.subprocess, "run", side_effect=self.run_command), \
             patch.object(update_plugin, "command", side_effect=self.command), \
             patch.object(update_plugin, "target", return_value=self.target), \
             patch.object(update_plugin.subprocess, "Popen", side_effect=AssertionError("No background process")), \
             patch.object(update_plugin.subprocess, "check_output", side_effect=AssertionError("No real CLI")):
            yield

    def update(self, **kwargs):
        return update_plugin.update("mock-repository", "candidate", None, self.state,
                                    "codex", expected_source="approved-source", **kwargs)

    def test_package_update_records_install_without_starting_a_runtime_or_daemon(self):
        with self.isolated():
            result = self.update()
        self.assertTrue(result["installed"])
        self.assertNotIn("legacy_installation", result)
        self.assertFalse(result["desktop_loaded"])
        self.assertEqual(result["phase"], "awaiting_desktop_reload")
        self.assertEqual(result["revision"], self.revision)
        self.assertEqual(result["release"], self.manifest)
        self.assertEqual(json.loads((self.state / "current.json").read_text()), result)
        self.assertFalse((self.state / "pending.json").exists())
        self.assertEqual(self.pending_at_install["phase"], "installing")
        self.assertFalse(self.pending_at_install["installed"])
        self.assertFalse(any("serve" in args or "stop" in args or "daemon" in args or "--config" in args
                             for args in self.commands))
        self.assertEqual(package_plugin.verify_package(Path(result["package"])), self.manifest)
        self.assertFalse(list(self.state.glob("backup*")))

    def test_repeated_upgrades_preserve_original_receipt_with_atomic_current_writes(self):
        import shutil
        self.state.mkdir()
        current = self.state / "current.json"
        original = {"config": "/nonexistent/custom/config.json", "backup": "old-backup",
                    "runtime": {"binding": ["original", 7]}, "unknown": None}
        current.write_text(json.dumps(original))
        for _ in range(3):
            shutil.rmtree(self.home, ignore_errors=True)
            shutil.rmtree(self.market, ignore_errors=True)
            with self.isolated(), patch.object(update_plugin.os, "replace", wraps=os.replace) as replace:
                result = self.update(reload=False)
            self.assertEqual(result["legacy_installation"], original)
            self.assertEqual(json.loads(current.read_text()), result)
            self.assertEqual(self.pending_at_install["legacy_installation"], original)
            writes = [call.args for call in replace.call_args_list if call.args[1] == current]
            self.assertEqual(len(writes), 2)
            self.assertTrue(all(source.parent == current.parent for source, _ in writes))
            self.assertFalse(Path(original["config"]).exists())

    def test_defer_reload_preserves_installed_but_unloaded_result(self):
        with self.isolated(), patch.object(update_plugin, "reload_plugins") as reload:
            result = self.update(reload=False)
        reload.assert_not_called()
        self.assertTrue(result["reload_deferred"])
        self.assertEqual(result["phase"], "awaiting_desktop_reload")
        self.assertTrue(result["installed"])
        self.assertFalse(result["desktop_loaded"])

    def desktop_connection(self, version=None):
        connection = MagicMock()
        connection.recv.side_effect = [json.dumps(item) for item in [
            {"method": "notification"}, {"id": 1, "result": {}},
            {"id": 2, "result": {}}, {"id": 3, "result": {}},
            {"id": 4, "result": {"data": [], "nextCursor": "next"}},
            {"id": 5, "result": {"data": [{
                "pluginId": "codex-session-board@session-board-rust",
                "serverInfo": {"name": "codex-session-board", "version": version or self.version},
                "tools": {"board_view": {}}}], "nextCursor": None}},
        ]]
        return connection

    def test_full_update_installs_marketplace_file_on_desktop_before_reload(self):
        connection = self.desktop_connection()
        websocket = SimpleNamespace(create_connection=MagicMock(return_value=connection))
        with self.isolated(), patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"), \
             patch.object(reload_plugins.socket, "socket") as transport, \
             patch.dict(sys.modules, {"websocket": websocket}):
            result = self.update()
        self.assertTrue(result["installed"])
        self.assertTrue(result["desktop_loaded"])
        self.assertEqual(result["phase"], "desktop_loaded")
        calls = [json.loads(call.args[0]) for call in connection.send.call_args_list]
        self.assertEqual([call["method"] for call in calls], ["initialize", "initialized",
            "plugin/install", "config/mcpServer/reload", "mcpServerStatus/list", "mcpServerStatus/list"])
        self.assertEqual(calls[2]["params"], {"marketplacePath": str(self.market / ".agents/plugins/marketplace.json"),
                                              "pluginName": "codex-session-board"})
        transport.return_value.__enter__.return_value.connect.assert_called_once_with(str(self.root / "desktop.sock"))
        connection.close.assert_called_once()

    def test_old_desktop_inventory_keeps_the_successful_install_record(self):
        connection = self.desktop_connection("old")
        with self.isolated(), patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"), \
             patch.object(reload_plugins.socket, "socket"), \
             patch.dict(sys.modules, {"websocket": SimpleNamespace(create_connection=lambda *a, **k: connection)}):
            result = self.update()
        self.assertTrue(result["installed"])
        self.assertFalse(result["desktop_loaded"])
        self.assertIn("version and tools", result["reload_error"])
        self.assertEqual(json.loads((self.state / "current.json").read_text()), result)

    def test_hash_mismatch_rejects_before_install(self):
        (self.source / "bin/codex-session-board").write_text("tampered")
        with self.isolated(), self.assertRaisesRegex(ValueError, "hashes differ"):
            self.update()
        self.assertFalse(any(args[:2] == ["codex", "plugin"] for args in self.commands))
        self.assertFalse((self.state / "current.json").exists())

    def test_wrong_platform_and_stale_channel_reject_before_install(self):
        for field, value, message in [("target", "aarch64-apple-darwin", "different operating"),
                                      ("source_commit", "stale", "not yet available")]:
            with self.subTest(field=field):
                manifest = dict(self.manifest, **{field: value})
                (self.source / "RELEASE.json").write_text(json.dumps(manifest))
                with self.isolated(), self.assertRaisesRegex(ValueError, message):
                    self.update()
        self.assertFalse(any(args[:2] == ["codex", "plugin"] for args in self.commands))

    def test_wrong_installed_git_revision_does_not_claim_install_success(self):
        self.installed_revision = "b" * 40
        with self.isolated(), self.assertRaisesRegex(ValueError, "Git revision differs"):
            self.update()
        self.assertFalse((self.state / "current.json").exists())
        self.assertFalse(json.loads((self.state / "pending.json").read_text())["installed"])

    def test_native_market_revision_must_match_even_without_cache_metadata(self):
        original = self.command
        def command(arguments, root=None):
            if arguments == ["git", "rev-parse", "HEAD"] and root == self.market:
                return "wrong-revision"
            return original(arguments, root)
        self.command = command
        with self.isolated(), self.assertRaisesRegex(ValueError, "marketplace Git revision differs"):
            self.update()
        self.assertFalse((self.state / "current.json").exists())

    def test_installed_cache_hash_mismatch_never_records_success(self):
        original = self.run_command
        def run(arguments, **kwargs):
            result = original(arguments, **kwargs)
            if arguments[:3] == ["codex", "plugin", "add"]:
                cache = self.home / "plugins/cache" / update_plugin.MARKET / "codex-session-board" / self.version
                (cache / "bin/codex-session-board").write_text("corrupted")
            return result
        self.run_command = run
        with self.isolated(), self.assertRaisesRegex(ValueError, "hashes differ"):
            self.update()
        self.assertFalse((self.state / "current.json").exists())

    def test_install_failure_preserves_previous_current_record(self):
        import subprocess
        self.state.mkdir()
        previous = {"installed": True, "release": "previous"}
        (self.state / "current.json").write_text(json.dumps(previous))
        original = self.run_command
        def run(arguments, **kwargs):
            if arguments[:3] == ["codex", "plugin", "add"]:
                raise subprocess.CalledProcessError(1, arguments)
            return original(arguments, **kwargs)
        self.run_command = run
        with self.isolated(), self.assertRaises(subprocess.CalledProcessError):
            self.update()
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)
        self.assertFalse(json.loads((self.state / "pending.json").read_text())["installed"])

    def test_lock_contention_never_fetches(self):
        import fcntl
        self.state.mkdir()
        with (self.state / "update.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.isolated(), self.assertRaises(BlockingIOError):
                self.update()
        self.assertEqual(self.commands, [])

    def test_atomic_record_preserves_previous_file_when_replace_fails(self):
        path = self.root / "current.json"
        path.write_text('{"previous": true}')
        with patch.object(update_plugin.os, "replace", side_effect=OSError("interrupted")):
            with self.assertRaises(OSError):
                update_plugin.write_record(path, {"installed": True})
        self.assertEqual(json.loads(path.read_text()), {"previous": True})
        self.assertEqual(list(self.root.glob("tmp*")), [])

    def test_cli_rejects_config_and_keeps_defer_reload(self):
        with patch.object(sys, "argv", ["update_plugin", "--repository", "mock", "--config", "private.json"]), \
             contextlib.redirect_stderr(io.StringIO()), patch.object(update_plugin, "update") as update:
            with self.assertRaises(SystemExit) as error:
                update_plugin.main()
            self.assertEqual(error.exception.code, 2)
            update.assert_not_called()
        with patch.object(sys, "argv", ["update_plugin", "--repository", "mock", "--state", str(self.state),
                                        "--defer-reload"]), \
             contextlib.redirect_stdout(io.StringIO()), patch.object(update_plugin, "update", return_value={}) as update:
            update_plugin.main()
        self.assertEqual(update.call_args.args[3:6], (self.state.resolve(), "codex", False))

    def test_desktop_discovery_uses_explicit_socket_and_linux_runtime_only(self):
        explicit = self.root / "explicit.sock"
        runtime = self.root / "runtime"
        default = runtime / "codex-desktop/app-server-bridge/app-server.sock"
        default.parent.mkdir(parents=True)
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(default))
            default.chmod(0o600)
            with patch.dict(os.environ, {"XDG_RUNTIME_DIR": str(runtime)}, clear=True), \
                 patch.object(reload_plugins.platform, "system", return_value="Linux"):
                self.assertEqual(reload_plugins.desktop_socket(), default)
                with patch.dict(os.environ, {"CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET": str(explicit)}):
                    self.assertIsNone(reload_plugins.desktop_socket())
                default.chmod(0o666)
                with self.assertRaisesRegex(RuntimeError, "private"):
                    reload_plugins.desktop_socket()

    def test_desktop_install_rejects_directory_instead_of_marketplace_file(self):
        with patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"):
            with self.assertRaisesRegex(ValueError, "marketplace.json file"):
                reload_plugins.reload_plugins("codex", self.root, self.version, self.source)


if __name__ == "__main__":
    unittest.main()
