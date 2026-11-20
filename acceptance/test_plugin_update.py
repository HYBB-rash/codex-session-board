import contextlib
import errno
import io
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

import websocket

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import board_update_job
import package_plugin
import plugin_publication
import reload_plugins
import update_plugin
import verify_plugin_upgrade


class PluginUpdateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        temporary_root = Path(self.temporary.name)
        fixture_root = temporary_root / "fixture"
        fixture_root.mkdir()
        fixture_alias = temporary_root / "alias"
        fixture_alias.symlink_to(fixture_root.resolve(), target_is_directory=True)
        self.root = fixture_alias.resolve()
        self.home = self.root / "home"
        self.state = self.root / "installation"
        self.source = self.root / "source"
        self.source.mkdir()
        self.market = self.home / ".tmp/marketplaces" / update_plugin.MARKET
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
        self.native_selections = []
        self.environment = patch.dict(os.environ, {"CODEX_HOME": str(self.home),
            "CODEX_THREAD_ID": "fixture-thread",
            "CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET": str(self.root / "absent.sock")})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def run_command(self, arguments, **kwargs):
        self.commands.append(arguments)
        environment = kwargs.get("env") or os.environ
        home = Path(environment["CODEX_HOME"])
        market = home / ".tmp/marketplaces" / update_plugin.MARKET
        if arguments[:2] == ["git", "checkout"]:
            shutil.copytree(self.source, kwargs["cwd"], dirs_exist_ok=True)
        if arguments[:4] == ["codex", "plugin", "marketplace", "add"]:
            shutil.copytree(self.source, market)
            (market / ".git").mkdir()
            reference = arguments[arguments.index("--ref") + 1] if "--ref" in arguments else None
            self.write_market_registration(reference, source=arguments[4], home=home)
        if arguments[:4] == ["codex", "plugin", "marketplace", "remove"]:
            shutil.rmtree(self.market)
            self.write_market_registration(None)
        if arguments[:3] == ["codex", "plugin", "add"]:
            pending = self.state / "pending.json"
            self.pending_at_install = json.loads(pending.read_text()) if pending.exists() else None
            cache = update_plugin.cache_directory(self.version, home=home)
            shutil.rmtree(cache.parent, ignore_errors=True)
            shutil.copytree(self.source, cache)
            (cache / ".codex-marketplace-install.json").write_text(json.dumps({
                "source_type": "git", "revision": self.installed_revision}))
            config = home / "config.toml"
            document = update_plugin.read_config(config)
            document.setdefault("plugins", {}).setdefault(update_plugin.PLUGIN_ID, {})["enabled"] = True
            config.write_text(update_plugin.tomlkit.dumps(document))
        return SimpleNamespace(returncode=0)

    def write_market_registration(self, reference, source="mock-repository", *, home=None, kind="git"):
        path = (home or self.home) / "config.toml"
        document = update_plugin.read_config(path)
        markets = document.setdefault("marketplaces", {})
        if reference is None and kind != "local":
            markets.pop(update_plugin.MARKET, None)
        else:
            markets[update_plugin.MARKET] = {"source": source, "source_type": kind, **({"ref": reference} if reference else {})}
        path.write_text(update_plugin.tomlkit.dumps(document))

    def command(self, arguments, root=None, *, home=None):
        self.commands.append(arguments)
        if arguments[:3] == ["codex", "plugin", "list"]:
            cache = update_plugin.cache_directory(home=home or self.home)
            versions = [path.name for path in cache.iterdir() if path.is_dir()] if cache.is_dir() else []
            version = max(versions, default=None)
            self.native_selections.append(version)
            return json.dumps({"installed": [{"pluginId": update_plugin.PLUGIN_ID, "installed": True,
                                             "version": version}] if version else []})
        if arguments[:4] == ["codex", "plugin", "marketplace", "list"]:
            home = home or self.home
            settings = update_plugin.market_settings(update_plugin.read_config(home / "config.toml"))
            markets = []
            if settings:
                market = home / ".tmp/marketplaces" / update_plugin.MARKET if settings["source_type"] == "git" else Path(settings["source"])
                if market.is_dir():
                    markets.append({"name": update_plugin.MARKET, "root": str(market),
                                    "marketplaceSource": {"sourceType": settings["source_type"], "source": settings["source"]}})
            return json.dumps({"marketplaces": markets})
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

    def test_verified_running_candidate_does_not_reinstall(self):
        with self.isolated():
            self.update()
            self.commands.clear()
            result = self.update(running_build=self.manifest["binary_sha256"])
        self.assertEqual(result["phase"], "up_to_date")
        self.assertFalse(any(args[:3] == ["codex", "plugin", "add"] for args in self.commands))
        self.assertFalse((self.state / "pending.json").exists())

    def test_installed_candidate_with_old_runtime_retries_only_reload(self):
        with self.isolated():
            self.update()
            self.commands.clear()
            with patch.object(update_plugin, "reload_plugins", return_value={
                    "phase": "awaiting_board_readback", "desktop_loaded": False}) as reload:
                result = self.update(running_build="old-build")
        reload.assert_called_once()
        self.assertEqual(result["phase"], "awaiting_board_readback")
        self.assertTrue(result["installed"])
        self.assertFalse(any(args[:3] == ["codex", "plugin", "add"] for args in self.commands))

    def test_receipt_repair_requires_the_native_selected_version(self):
        with self.isolated():
            installed = self.update(reload=False)
        cache = update_plugin.cache_directory()
        shutil.copytree(cache / self.version, cache / "1.2.4")
        current = self.state / "current.json"
        stale = dict(installed, release=dict(self.manifest, version="1.2.4"))
        current.write_text(json.dumps(stale))
        pending = self.state / "pending.json"
        pending.write_text(json.dumps(dict(stale, installed=False)))
        self.commands.clear()
        self.native_selections.clear()
        with self.isolated():
            result = self.update(running_build=self.manifest["binary_sha256"], reload=False)
        self.assertEqual(result["phase"], "awaiting_desktop_reload")
        self.assertEqual(json.loads(current.read_text()), result)
        self.assertFalse(pending.exists())
        self.assertEqual(self.native_selections, ["1.2.4", self.version, self.version])
        self.assertTrue(any(args[:3] == ["codex", "plugin", "add"] for args in self.commands))
        self.assertFalse((cache / "1.2.4").exists())

    def desktop_connection(self, version=None, reload_error=None):
        connection = MagicMock()
        responses = [json.dumps(item) for item in [
            {"method": "notification"}, {"id": 1, "result": {}},
            {"id": 0, "result": {"thread": {"status": {"type": "idle"}}}},
            {"id": 3, "result": {}},
            {"id": 4, "result": {"data": [], "nextCursor": "next"}},
            {"id": 5, "result": {"data": [{
                "pluginId": "codex-session-board@session-board-rust",
                "serverInfo": {"name": "codex-session-board", "version": version or self.version},
                "tools": {"board_view": {}}}], "nextCursor": None}},
            {"id": 6, "result": {"content": [], "structuredContent": {"sessions": []}}},
        ]]
        connection.recv.side_effect = responses if reload_error is None else responses[:3] + [reload_error]
        return connection

    def test_full_update_reloads_existing_native_install_without_install_rpc(self):
        connection = self.desktop_connection()
        with self.isolated(), patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"), \
             patch.object(reload_plugins.socket, "socket") as transport, \
             patch.object(websocket, "create_connection", return_value=connection):
            result = self.update()
        self.assertTrue(result["installed"])
        self.assertTrue(result["desktop_loaded"])
        self.assertEqual(result["phase"], "desktop_loaded")
        calls = [json.loads(call.args[0]) for call in connection.send.call_args_list]
        self.assertEqual([call["method"] for call in calls], ["initialize", "initialized",
            "thread/read", "config/mcpServer/reload", "mcpServerStatus/list",
            "mcpServerStatus/list", "mcpServer/tool/call"])
        self.assertTrue((self.market / ".agents/plugins/marketplace.json").is_file())
        transport.return_value.__enter__.return_value.connect.assert_called_once_with(str(self.root / "desktop.sock"))
        connection.close.assert_called_once()

    def test_old_desktop_inventory_keeps_the_successful_install_record(self):
        connection = self.desktop_connection("old")
        with self.isolated(), patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"), \
             patch.object(reload_plugins.socket, "socket"), \
             patch.object(websocket, "create_connection", return_value=connection):
            result = self.update()
        self.assertTrue(result["installed"])
        self.assertFalse(result["desktop_loaded"])
        self.assertIn("version and tools", result["reload_error"])
        self.assertEqual(json.loads((self.state / "current.json").read_text()), result)

    def assert_worker_keeps_install_after_reload_error(self, error):
        connection = self.desktop_connection(reload_error=error)
        with update_plugin.update_lock(self.state) as lock:
            snapshot = self.state / "updater-fixture"
            snapshot.mkdir()
            with self.isolated(), patch.object(board_update_job, "__file__", str(snapshot / "board_update_job.py")), \
                 patch.object(reload_plugins, "desktop_socket", return_value=self.root / "desktop.sock"), \
                 patch.object(reload_plugins.socket, "socket"), \
                 patch.object(websocket, "create_connection", return_value=connection):
                board_update_job.work(self.state, "codex", "old-build", lock.fileno())
        self.assertEqual(json.loads(connection.send.call_args.args[0])["method"], "config/mcpServer/reload")
        connection.close.assert_called_once()
        current = json.loads((self.state / "current.json").read_text())
        status = json.loads((self.state / board_update_job.STATUS).read_text())
        self.assertTrue(current["installed"])
        self.assertFalse(current["desktop_loaded"])
        self.assertEqual(current["phase"], "awaiting_desktop_reload")
        self.assertEqual(current["release"], self.manifest)
        self.assertIn(type(error).__name__, current["reload_error"])
        self.assertIn(str(error), current["reload_error"])
        self.assertEqual(status, {"phase": "awaiting_desktop_reload", "version": self.version,
            "build": self.manifest["binary_sha256"], "installed": True, "error": current["reload_error"]})
        self.assertFalse((self.state / "pending.json").exists())
        self.assertFalse(snapshot.exists())

    def test_worker_records_install_when_reload_connection_closes(self):
        self.assert_worker_keeps_install_after_reload_error(
            websocket.WebSocketConnectionClosedException("fixture desktop disconnected"))

    def test_worker_records_install_when_reload_connection_times_out(self):
        self.assert_worker_keeps_install_after_reload_error(
            websocket.WebSocketTimeoutException("fixture desktop did not reply"))

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
        with self.isolated(), self.assertRaisesRegex(RuntimeError, "Git revision differs"):
            self.update()
        self.assertFalse((self.state / "current.json").exists())
        self.assertFalse(json.loads((self.state / "pending.json").read_text())["installed"])

    def test_native_market_revision_must_match_even_without_cache_metadata(self):
        original = self.command
        def command(arguments, root=None, *, home=None):
            if arguments == ["git", "rev-parse", "HEAD"] and root is not None and root.name == update_plugin.MARKET:
                return "wrong-revision"
            return original(arguments, root, home=home)
        self.command = command
        with self.isolated(), self.assertRaisesRegex(RuntimeError, "未确认本次候选市场的来源和提交"):
            self.update()
        self.assertFalse(any(args[:3] == ["codex", "plugin", "add"] for args in self.commands))
        self.assertFalse((self.state / "current.json").exists())

    def test_installed_cache_hash_mismatch_never_records_success(self):
        original = self.run_command
        def run(arguments, **kwargs):
            result = original(arguments, **kwargs)
            if arguments[:3] == ["codex", "plugin", "add"]:
                cache = update_plugin.cache_directory(self.version, home=Path(kwargs["env"]["CODEX_HOME"]))
                (cache / "bin/codex-session-board").write_text("corrupted")
            return result
        self.run_command = run
        with self.isolated(), self.assertRaisesRegex(RuntimeError, "hashes differ"):
            self.update()
        self.assertFalse((self.state / "current.json").exists())

    def test_install_failure_preserves_previous_current_record(self):
        self.state.mkdir()
        previous = {"installed": True, "release": "previous"}
        (self.state / "current.json").write_text(json.dumps(previous))
        original = self.run_command
        def run(arguments, **kwargs):
            if arguments[:3] == ["codex", "plugin", "add"]:
                raise subprocess.CalledProcessError(1, arguments)
            return original(arguments, **kwargs)
        self.run_command = run
        with self.isolated(), self.assertRaisesRegex(RuntimeError, "未留下市场注册"):
            self.update()
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)
        self.assertFalse(json.loads((self.state / "pending.json").read_text())["installed"])

    def test_marketplace_timeout_restores_previous_registration_and_cache(self):
        with self.isolated():
            previous = self.update()
        self.write_market_registration("baseline")
        (self.market / "RELEASE.json").unlink()
        (self.market / "source-only").write_text("default branch, no release manifest")
        original = self.run_command
        attempts = []

        def run(arguments, **kwargs):
            if arguments[:4] == ["codex", "plugin", "marketplace", "add"]:
                attempts.append(arguments)
                if len(attempts) == 1:
                    raise subprocess.TimeoutExpired(arguments, 120)
            return original(arguments, **kwargs)

        self.run_command = run
        with self.isolated():
            with self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场及缓存"):
                self.update()
            self.assertEqual(update_plugin.cached_package("codex", self.version, self.revision), self.manifest)
        self.assertEqual([arguments[-1] for arguments in attempts], [self.revision])
        self.assertEqual(tomllib.loads((self.home / "config.toml").read_text())["marketplaces"]
                         [update_plugin.MARKET]["ref"], "baseline")
        self.assertEqual((self.market / "source-only").read_text(), "default branch, no release manifest")
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)

    def test_orphan_cache_survives_candidate_install_and_failed_readback(self):
        cache = update_plugin.cache_directory(self.version)
        with self.isolated():
            previous = self.update(reload=False)
        shutil.rmtree(self.market)
        old = cache.with_name("old-version")
        cache.rename(old)
        old_hashes = package_plugin.hashes(old)
        config = self.home / "config.toml"
        config.write_text(f'[plugins."{update_plugin.PLUGIN_ID}"]\nenabled = true\n')
        original = self.run_command

        def run(arguments, **kwargs):
            if arguments[:3] == ["codex", "plugin", "add"]:
                result = original(arguments, **kwargs)
                staged = update_plugin.cache_directory(self.version, home=Path(kwargs["env"]["CODEX_HOME"]))
                self.assertFalse(staged.with_name(old.name).exists())
                (staged / "RELEASE.json").write_text("null")
                return result
            return original(arguments, **kwargs)

        self.run_command = run
        with self.isolated(), self.assertRaisesRegex(RuntimeError, "manifest must be an object.*已恢复原缓存"):
            self.update(reload=False)
        self.assertFalse(self.market.exists())
        self.assertFalse(cache.exists())
        self.assertEqual(package_plugin.hashes(old), old_hashes)
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)

    def test_unexpected_install_and_rollback_errors_keep_the_recovery_copy(self):
        cache = update_plugin.cache_directory(self.version)
        with self.isolated():
            previous = self.update(reload=False)
        shutil.rmtree(self.market)
        self.write_market_registration(None)
        expected = package_plugin.hashes(cache)
        verify = update_plugin.verify_candidate_cache

        def fail(codex, manifest, revision, *, home=None):
            verify(codex, manifest, revision, home=home)
            if home is None:
                (cache / "concurrent-file").write_text("external cache modification")
                raise TypeError("unexpected verifier failure")

        with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
             self.assertRaisesRegex(RuntimeError, "unexpected verifier failure.*插件缓存冲突.*备份保留在"):
            self.update(reload=False)
        backup, = cache.parent.parent.glob(".board-rollback-*")
        saved = update_plugin.cache_directory(self.version, home=backup / "native")
        self.assertEqual(package_plugin.hashes(saved), expected)
        self.assertEqual((cache / "concurrent-file").read_text(), "external cache modification")
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)

    def test_rollback_does_not_replace_live_config_after_a_late_write(self):
        config = self.home / "config.toml"
        self.home.mkdir()
        config.write_text(f'[plugins."{update_plugin.PLUGIN_ID}"]\nenabled = false\n')
        verify = update_plugin.verify_candidate_cache
        atomic_rename = plugin_publication.atomic_rename

        def fail(codex, manifest, revision, *, home=None):
            verify(codex, manifest, revision, home=home)
            if home is None:
                raise ValueError("cache check failed")

        def save_history(source, destination, *, exchange):
            if source == config:
                with config.open("a") as writer:
                    writer.write("\n[history]\npersistence = 'none'\n")
            return atomic_rename(source, destination, exchange=exchange)

        with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
             patch.object(plugin_publication, "atomic_rename", side_effect=save_history), \
             self.assertRaisesRegex(RuntimeError, "cache check failed.*配置冲突.*备份保留在"):
            self.update(reload=False)
        document = update_plugin.read_config(config)
        self.assertFalse(update_plugin.plugin_enabled(document))
        self.assertEqual(document.unwrap()["history"], {"persistence": "none"})

    def test_marketplace_timeout_preserves_registration_from_another_writer(self):
        original = self.run_command

        def run(arguments, **kwargs):
            if arguments[:4] == ["codex", "plugin", "marketplace", "add"]:
                self.register_foreign()
                raise subprocess.TimeoutExpired("marketplace add", 120)
            return original(arguments, **kwargs)

        self.run_command = run
        with self.isolated():
            with self.assertRaisesRegex(RuntimeError, "市场注册冲突.*已保留当前来源.*备份保留在"):
                self.update(reload=False)
            self.assert_foreign_market()
        self.assertFalse((self.state / "current.json").exists())

    def register_foreign(self):
        foreign = self.root / "foreign"
        foreign.mkdir(exist_ok=True)
        self.write_market_registration(None, source=str(foreign), kind="local")

    def assert_foreign_market(self):
        registration = update_plugin.market_registration("codex")
        self.assertEqual(registration["marketplaceSource"], {"sourceType": "local", "source": str(self.root / "foreign")})

    def test_late_market_registration_survives_update_and_rollback(self):
        config = self.home / "config.toml"
        config.parent.mkdir()
        config.write_text("# user settings\n")
        atomic_rename = plugin_publication.atomic_rename

        def register_during_exchange(source, destination, *, exchange):
            if destination == config:
                self.register_foreign()
            return atomic_rename(source, destination, exchange=exchange)

        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=register_during_exchange):
            with self.assertRaisesRegex(RuntimeError, "配置冲突.*市场注册冲突.*备份保留在"):
                self.update(reload=False)
            self.assert_foreign_market()
            self.assertFalse(self.market.exists())
            self.assertFalse(update_plugin.cache_directory().exists())

    def test_rollback_preserves_foreign_installation_and_enabled_setting(self):
        cache = update_plugin.cache_directory(self.version)
        verify = update_plugin.verify_candidate_cache

        def fail(codex, manifest, revision, *, home=None):
            verify(codex, manifest, revision, home=home)
            if home is None:
                self.register_foreign()
                (cache / "foreign-file").write_text("external installation")
                raise ValueError("failed final candidate check")

        with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail):
            with self.assertRaisesRegex(RuntimeError, "failed final candidate check.*市场注册冲突.*备份保留在"):
                self.update(reload=False)
            self.assert_foreign_market()
        self.assertEqual((cache / "foreign-file").read_text(), "external installation")
        self.assertTrue(update_plugin.plugin_enabled(update_plugin.read_config(self.home / "config.toml")))

    def test_registered_only_final_validation_failure_restores_absent_enabled(self):
        with self.isolated():
            self.update(reload=False)
        config = self.home / "config.toml"
        current = update_plugin.read_config(config)
        current.pop("plugins")
        config.write_text(update_plugin.tomlkit.dumps(current))
        shutil.rmtree(update_plugin.cache_directory())
        verify = update_plugin.verify_candidate_cache

        def fail(codex, manifest, revision, *, home=None):
            verify(codex, manifest, revision, home=home)
            if home is None:
                raise ValueError("failed final candidate check")

        with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
             self.assertRaisesRegex(RuntimeError, "failed final candidate check.*已恢复上一个插件市场"):
            self.update(reload=False)
        self.assertIsNone(update_plugin.plugin_enabled(update_plugin.read_config(config)))
        self.assertFalse(update_plugin.cache_directory().exists())

    def test_concurrent_disable_at_config_exchange_is_not_overwritten(self):
        config = self.home / "config.toml"
        config.parent.mkdir()
        config.write_text("# user settings\n")
        atomic_rename = plugin_publication.atomic_rename

        def save_disabled(source, destination, *, exchange):
            if destination == config:
                with config.open("a") as writer:
                    writer.write(f'\n[plugins."{update_plugin.PLUGIN_ID}"]\nenabled = false\n')
            return atomic_rename(source, destination, exchange=exchange)

        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=save_disabled), \
             self.assertRaisesRegex(RuntimeError, "配置冲突.*插件启用状态冲突.*备份保留在"):
            self.update(reload=False)
        self.assertFalse(update_plugin.plugin_enabled(update_plugin.read_config(config)))
        self.assertFalse(update_plugin.cache_directory().exists())

    def installation_snapshot(self):
        return (
            (self.home / "config.toml").read_bytes(),
            plugin_publication.fingerprint(self.market),
            plugin_publication.fingerprint(update_plugin.cache_directory()),
            (self.state / "current.json").read_bytes(),
        )

    def test_sigint_after_each_exchange_restores_verified_installation(self):
        with self.isolated():
            self.update(reload=False)
        snapshot = self.installation_snapshot()
        atomic_rename = plugin_publication.atomic_rename
        cache = update_plugin.cache_directory()
        config = self.home / "config.toml"
        for interrupted_path in (self.market, cache, config):
            with self.subTest(path=interrupted_path):
                def interrupt(source, destination, *, exchange):
                    atomic_rename(source, destination, exchange=exchange)
                    if destination == interrupted_path:
                        signal.raise_signal(signal.SIGINT)

                with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=interrupt), \
                     self.assertRaisesRegex(RuntimeError, "SIGINT.*已恢复上一个插件市场及缓存"):
                    self.update(reload=False)
                self.assertEqual(self.installation_snapshot(), snapshot)
                self.assertFalse(list(cache.parent.glob(".board-rollback-*")))
                retained = list(self.home.glob(".board-rollback-config-*/config.toml"))
                self.assertEqual(len(retained), int(interrupted_path == config))

    def test_write_to_displaced_config_is_detected_during_readback(self):
        with self.isolated():
            self.update(reload=False)
        snapshot = self.installation_snapshot()
        config = self.home / "config.toml"
        external = snapshot[0].replace(b"enabled = true", b"enabled = false") + b'\n[history]\npersistence = "none"\n'
        verify = update_plugin.verify_candidate_cache
        with config.open("r+b") as writer:
            def write_during_readback(codex, manifest, revision, *, home=None):
                verify(codex, manifest, revision, home=home)
                if home is None:
                    writer.seek(0)
                    writer.write(external)
                    writer.truncate()
                    writer.flush()
                    os.fsync(writer.fileno())

            with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=write_during_readback), \
                 self.assertRaisesRegex(RuntimeError, "配置冲突.*插件启用状态冲突.*备份保留在"):
                self.update(reload=False)
        self.assertEqual(self.installation_snapshot(), (external, *snapshot[1:]))

    def test_config_link_retargeting_restores_both_original_files(self):
        with self.isolated():
            self.update(reload=False)
        snapshot = self.installation_snapshot()
        config = self.home / "config.toml"
        original, replacement = self.root / "first.toml", self.root / "second.toml"
        config.rename(original)
        replacement.write_bytes(original.read_bytes())
        config.symlink_to(original)
        atomic_rename = plugin_publication.atomic_rename

        def retarget(source, destination, *, exchange):
            if destination == original:
                other_link = config.with_name("new-link")
                other_link.symlink_to(replacement)
                other_link.replace(config)
            return atomic_rename(source, destination, exchange=exchange)

        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=retarget), \
             self.assertRaisesRegex(RuntimeError, "配置文件路径已变化.*备份保留在"):
            self.update(reload=False)
        self.assertEqual(config.resolve(), replacement)
        self.assertEqual(original.read_bytes(), snapshot[0])
        self.assertEqual(self.installation_snapshot(), snapshot)

    def test_rollback_reports_retargeted_cache_entry_and_leaves_new_target_alone(self):
        cache = self.home / "plugins/cache"
        cache.parent.mkdir(parents=True)
        original, replacement = self.root / "cache-a", self.root / "cache-b"
        original.mkdir()
        replacement.mkdir()
        cache.symlink_to(original, target_is_directory=True)
        with self.isolated():
            self.update(reload=False)
        old_plugin = update_plugin.cache_directory().resolve()
        old_cache = plugin_publication.fingerprint(old_plugin)
        snapshot = self.installation_snapshot()
        atomic_rename = plugin_publication.atomic_rename

        def retarget(source, destination, *, exchange):
            atomic_rename(source, destination, exchange=exchange)
            if destination == self.home / "config.toml" and cache.resolve() == original:
                link = cache.with_name("cache-replacement")
                link.symlink_to(replacement, target_is_directory=True)
                link.replace(cache)

        self.revision = self.installed_revision = "b" * 40
        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=retarget), \
             self.assertRaisesRegex(RuntimeError, "安装目录已变化.*备份保留在") as failed:
            self.update(reload=False)
        self.assertNotIn("已恢复上一个插件市场及缓存", str(failed.exception))
        self.assertEqual(cache.resolve(), replacement)
        self.assertEqual(list(replacement.iterdir()), [])
        self.assertEqual(plugin_publication.fingerprint(old_plugin), old_cache)
        self.assertEqual((self.state / "current.json").read_bytes(), snapshot[3])
        self.assertTrue(list(self.market.parent.glob(".board-rollback-market-*")))

    def test_interleaved_dotted_settings_round_trip_with_native_install(self):
        self.market.mkdir(parents=True)
        config = self.home / "config.toml"
        original = (
            b'\xef\xbb\xbf# retained\n'
            b'marketplaces.session-board-rust.source = "mock-repository"\n'
            b'plugins."codex-session-board@session-board-rust".enabled = false\n'
            b'marketplaces.session-board-rust.source_type = "git"\n'
            b'marketplaces.session-board-rust.ref = "previous"\n'
            b'notice.hide_rate_limit_model_nudge = true\n'
        )
        config.write_bytes(original)
        expected = update_plugin.read_config(config).unwrap()
        expected["marketplaces"][update_plugin.MARKET]["ref"] = self.revision
        with self.isolated():
            result = self.update(reload=False)
        self.assertEqual(result["revision"], self.revision)
        self.assertTrue(config.read_bytes().startswith(b'\xef\xbb\xbf# retained\n'))
        self.assertEqual(tomllib.loads(config.read_text(encoding="utf-8-sig")), expected)

    def test_invalid_staged_config_never_publishes_installation(self):
        with self.isolated():
            self.update(reload=False)
        snapshot = self.installation_snapshot()
        prepare = update_plugin.updated_config

        def invalidate(contents, settings):
            _, expected = prepare(contents, settings)
            return "[marketplaces]\n", expected

        self.revision = self.installed_revision = "b" * 40
        with self.isolated(), patch.object(update_plugin, "updated_config", side_effect=invalidate), \
             patch.object(plugin_publication.Publication, "publish") as publish, \
             self.assertRaisesRegex(RuntimeError, "暂存配置读回与预期不一致"):
            self.update(reload=False)
        publish.assert_not_called()
        self.assertEqual(self.installation_snapshot(), snapshot)

    def test_linked_config_is_staged_beside_target_and_preserves_bom(self):
        config = self.home / "config.toml"
        self.home.mkdir()
        external_config = self.root / "settings" / "actual.toml"
        external_config.parent.mkdir()
        original = b'\xef\xbb\xbf# settings\n[history]\npersistence = "none"\n'
        external_config.write_bytes(original)
        config.symlink_to(external_config)
        atomic_rename = plugin_publication.atomic_rename
        config_publications = []

        def require_config_filesystem(source, destination, *, exchange):
            if destination == external_config:
                if source.parent.parent != external_config.parent:
                    raise OSError("cross-device configuration swap")
                config_publications.append(source)
            atomic_rename(source, destination, exchange=exchange)

        verify = update_plugin.verify_candidate_cache

        def fail(codex, manifest, revision, *, home=None):
            verify(codex, manifest, revision, home=home)
            if home is None:
                raise ValueError("failed final candidate check")

        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=require_config_filesystem):
            with patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
                 self.assertRaisesRegex(RuntimeError, "failed final candidate check.*未留下市场注册"):
                self.update(reload=False)
            self.assertEqual(external_config.read_bytes(), original)
            self.assertFalse(self.market.exists())
            self.assertFalse(update_plugin.cache_directory().exists())
            self.assertFalse((self.state / "current.json").exists())
            result = self.update(reload=False)
        self.assertEqual(config.resolve(), external_config)
        self.assertTrue(external_config.read_bytes().startswith(original))
        self.assertEqual(result["revision"], self.revision)
        failed_staging, saved_original = config_publications
        self.assertTrue(failed_staging.exists())
        self.assertEqual(saved_original.read_bytes(), original)

    def test_rollback_reports_backup_for_writer_holding_candidate_config(self):
        with self.isolated():
            self.update(reload=False)
        snapshot = self.installation_snapshot()
        config = self.home / "config.toml"
        cache = update_plugin.cache_directory()
        verify = update_plugin.verify_candidate_cache
        atomic_rename = plugin_publication.atomic_rename
        history = b'\n[history]\npersistence = "none"\n'
        with contextlib.ExitStack() as writer_scope:
            writer = None

            def fail(codex, manifest, revision, *, home=None):
                nonlocal writer
                verify(codex, manifest, revision, home=home)
                if home is None:
                    writer = writer_scope.enter_context(config.open("ab"))
                    raise ValueError("failed final candidate check")

            def save_during_cache_restore(source, destination, *, exchange):
                atomic_rename(source, destination, exchange=exchange)
                if source == cache:
                    writer.write(history)
                    writer.flush()
                    os.fsync(writer.fileno())

            with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
                 patch.object(plugin_publication, "atomic_rename", side_effect=save_during_cache_restore), \
                 self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场及缓存.*换下的配置保留在") as failure:
                self.update(reload=False)
        retained, = self.home.glob(".board-rollback-config-*/config.toml")
        self.assertIn(str(retained), str(failure.exception))
        self.assertTrue(retained.read_bytes().endswith(history))
        self.assertEqual(self.installation_snapshot(), snapshot)

    def test_install_receipt_precedes_cleanup_and_retry_repairs_stale_receipts(self):
        with self.isolated():
            previous = self.update(reload=False)
        config = self.home / "config.toml"
        current = self.state / "current.json"
        pending = self.state / "pending.json"
        self.revision = self.installed_revision = "b" * 40
        rmtree = shutil.rmtree

        def interrupt_after_cleanup(path, *args, **kwargs):
            rmtree(path, *args, **kwargs)
            if Path(path).name.startswith(".board-rollback-"):
                record = json.loads(current.read_text())
                self.assertTrue(record["installed"])
                self.assertEqual(record["revision"], self.revision)
                self.assertEqual(record["phase"], "awaiting_desktop_reload")
                self.assertFalse(pending.exists())
                signal.raise_signal(signal.SIGINT)

        with self.isolated(), patch.object(shutil, "rmtree", side_effect=interrupt_after_cleanup):
            result = self.update(reload=False)
        self.assertIn("收尾被中断", result["reload_error"])
        self.assertEqual(update_plugin.market_settings(update_plugin.read_config(config))["ref"], self.revision)
        self.assertEqual(json.loads(current.read_text()), result)
        self.assertFalse(pending.exists())

        current.write_text(json.dumps(previous))
        pending.write_text(json.dumps(dict(result, phase="installing", installed=False)))
        self.commands.clear()
        with self.isolated():
            recovered = self.update(running_build=self.manifest["binary_sha256"])
        self.assertEqual(recovered["phase"], "up_to_date")
        self.assertEqual(recovered["revision"], self.revision)
        self.assertEqual(json.loads(current.read_text()), recovered)
        self.assertFalse(pending.exists())
        self.assertFalse(any(args[:3] == ["codex", "plugin", "add"] for args in self.commands))

    def test_market_and_cache_on_different_filesystems_publish_and_restore(self):
        mount = Path("/dev/shm")
        if not mount.is_dir() or mount.stat().st_dev == self.root.stat().st_dev:
            self.skipTest("requires a second writable filesystem at /dev/shm")
        with tempfile.TemporaryDirectory(prefix="board-market-cache-", dir=mount) as cache_root:
            self.home.mkdir()
            (self.home / "plugins").mkdir()
            (self.home / "plugins/cache").symlink_to(cache_root, target_is_directory=True)
            with self.isolated():
                self.update(reload=False)
            snapshot = self.installation_snapshot()
            verify = update_plugin.verify_candidate_cache

            def fail(codex, manifest, revision, *, home=None):
                verify(codex, manifest, revision, home=home)
                if home is None:
                    raise ValueError("failed final candidate check")

            self.revision = self.installed_revision = "b" * 40
            with self.isolated(), patch.object(update_plugin, "verify_candidate_cache", side_effect=fail), \
                 self.assertRaisesRegex(RuntimeError, "failed final candidate check.*已恢复上一个插件市场及缓存"):
                self.update(reload=False)
            self.assertEqual(self.installation_snapshot(), snapshot)
            with self.isolated():
                self.assertEqual(self.update(reload=False)["revision"], self.revision)
            self.assertEqual(self.market.stat().st_dev, self.root.stat().st_dev)
            self.assertEqual(update_plugin.cache_directory().stat().st_dev, mount.stat().st_dev)
            self.assertFalse(list(self.market.parent.glob(".board-rollback-market-*")))

    def test_market_staging_stays_on_its_mount_even_when_device_ids_match(self):
        atomic_rename = plugin_publication.atomic_rename
        market_moves = []

        def separate_mounts(source, destination, *, exchange):
            if destination == self.market:
                self.assertEqual(source.stat().st_dev, destination.parent.stat().st_dev)
                if source.parent.parent != destination.parent:
                    raise OSError(errno.EXDEV, os.strerror(errno.EXDEV))
                market_moves.append(source)
            atomic_rename(source, destination, exchange=exchange)

        with self.isolated(), patch.object(plugin_publication, "atomic_rename", side_effect=separate_mounts):
            self.assertTrue(self.update(reload=False)["installed"])
            self.revision = self.installed_revision = "b" * 40
            self.assertEqual(self.update(reload=False)["revision"], self.revision)
        self.assertEqual(len(market_moves), 2)

    def test_native_recovery_verifier_rejects_an_install_that_did_not_prune_old_cache(self):
        old_cache = update_plugin.cache_directory("old-version")
        shutil.copytree(self.source, old_cache)
        candidate = update_plugin.cache_directory(self.version)
        shutil.copytree(self.source, candidate)
        self.state.mkdir()
        (self.state / "current.json").write_text("{}")
        previous = {"version": "old-version", "binary_sha256": "old-build"}

        def install(**kwargs):
            try:
                update_plugin.cached_package("codex", self.version, self.revision)
            except ValueError as error:
                raise RuntimeError("已恢复原缓存") from error

        with patch.object(update_plugin, "remove_market"), \
             patch.object(verify_plugin_upgrade, "market_registration", return_value=None), \
             patch.object(verify_plugin_upgrade, "cached_package", side_effect=lambda *args, **kwargs:
                          package_plugin.verify_package(candidate)), \
             patch.object(verify_plugin_upgrade, "app_server") as host, \
             self.assertRaisesRegex(ValueError, "candidate did not replace the old cache"):
            verify_plugin_upgrade.verify_orphan_recovery("codex", self.root, self.state, {}, previous, install)
        host.assert_not_called()

    def exercise_local_market(self, root, initial_enabled, install_failure, cached_source=False):
        root.mkdir()
        old_cache = update_plugin.cache_directory("legacy")
        legacy = old_cache if cached_source else root / "legacy"
        shutil.copytree(self.source, legacy)
        (legacy / "RELEASE.json").unlink()
        legacy_hashes = package_plugin.hashes(legacy)
        plugin_key = f'plugins."{update_plugin.PLUGIN_ID}"'
        config = self.home / "config.toml"
        config.parent.mkdir(parents=True, exist_ok=True)
        multiline_history = 'history = {\n persistence = "none",\n}\n'
        config.write_text('# user comment\nmodel_instructions_file = "instructions.md"\n'
                          + multiline_history + '[analytics]\nenabled = false\n')
        (self.home / "instructions.md").write_text("Test instructions")
        if initial_enabled is not None:
            if not cached_source:
                shutil.copytree(legacy, old_cache)
            with config.open("a") as settings:
                settings.write(f"\n[{plugin_key}]\nenabled = {str(initial_enabled).lower()}\n")
        self.write_market_registration(None, source=str(legacy), kind="local")
        initial = config.read_bytes()
        with self.isolated():
            if install_failure == "cache":
                with patch.object(update_plugin, "verify_candidate_cache", side_effect=ValueError("cache check failed")):
                    with self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场"):
                        update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
            elif install_failure == "market":
                with patch.object(update_plugin, "add_market", side_effect=subprocess.TimeoutExpired("marketplace add", 120)), \
                     patch.object(update_plugin, "add_plugin") as install, \
                     self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场"):
                    update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
                install.assert_not_called()
            else:
                update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
            registration = update_plugin.market_registration("codex")["marketplaceSource"]
        self.assertIn('model_instructions_file = "instructions.md"\n' + multiline_history, config.read_text())
        if install_failure:
            self.assertEqual(registration, {"sourceType": "local", "source": str(legacy)})
            self.assertFalse(update_plugin.cache_directory(self.version).exists())
            self.assertEqual(old_cache.exists(), initial_enabled is not None)
            if old_cache.exists():
                self.assertEqual(package_plugin.hashes(old_cache), legacy_hashes)
            self.assertEqual(config.read_bytes(), initial)
        else:
            self.assertEqual(registration, {"sourceType": "git", "source": "mock-repository"})
            self.assertEqual(update_plugin.verify_package(update_plugin.cache_directory(self.version), installed=True), self.manifest)
        shutil.rmtree(self.home)

    def test_legacy_local_market_without_release_manifest_can_upgrade_or_restore(self):
        for failure in (None, "cache"):
            with self.subTest(failure=failure):
                self.exercise_local_market(self.root / f"legacy-{failure}", True, failure)

    def test_local_market_inside_old_cache_is_restored_after_cache_rollback(self):
        self.exercise_local_market(self.root / "cached-market", True, "cache", cached_source=True)

    def test_rollback_keeps_registered_only_and_disabled_plugins_in_their_prior_state(self):
        for enabled in (None, False):
            for failure in ("market", "cache"):
                with self.subTest(enabled=enabled, failure=failure):
                    self.exercise_local_market(self.root / f"{enabled}-{failure}", enabled, failure)

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
