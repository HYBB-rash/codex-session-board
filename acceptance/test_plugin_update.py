import contextlib
import io
import json
import os
import shutil
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
        self.environment = patch.dict(os.environ, {"CODEX_HOME": str(self.home),
            "CODEX_THREAD_ID": "fixture-thread",
            "CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET": str(self.root / "absent.sock")})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def run_command(self, arguments, **kwargs):
        self.commands.append(arguments)
        if arguments[:2] == ["git", "checkout"]:
            shutil.copytree(self.source, kwargs["cwd"], dirs_exist_ok=True)
        if arguments[:4] == ["codex", "plugin", "marketplace", "add"]:
            shutil.copytree(self.source, self.market)
            (self.market / ".git").mkdir()
            reference = arguments[arguments.index("--ref") + 1] if "--ref" in arguments else None
            self.write_market_registration(reference, source=arguments[4])
        if arguments[:4] == ["codex", "plugin", "marketplace", "remove"]:
            shutil.rmtree(self.market)
            self.write_market_registration(None)
        if arguments[:3] == ["codex", "plugin", "add"]:
            self.pending_at_install = json.loads((self.state / "pending.json").read_text())
            cache = self.home / "plugins/cache" / update_plugin.MARKET / "codex-session-board" / self.version
            shutil.copytree(self.source, cache, dirs_exist_ok=True)
            (cache / ".codex-marketplace-install.json").write_text(json.dumps({
                "source_type": "git", "revision": self.installed_revision}))
            environment = kwargs.get("env") or os.environ
            config = Path(environment["CODEX_HOME"]) / "config.toml"
            document = update_plugin.read_config(config)
            document.setdefault("plugins", {}).setdefault(update_plugin.PLUGIN_ID, {})["enabled"] = True
            config.write_text(update_plugin.tomlkit.dumps(document))
        return SimpleNamespace(returncode=0)

    def write_market_registration(self, reference, source="mock-repository"):
        path = self.home / "config.toml"
        document = update_plugin.tomlkit.parse(update_plugin.config_text(path))
        markets = document.setdefault("marketplaces", {})
        if reference is None:
            markets.pop(update_plugin.MARKET, None)
        else:
            markets[update_plugin.MARKET] = {"source": source, "source_type": "git", "ref": reference}
        path.write_text(update_plugin.tomlkit.dumps(document))

    def command(self, arguments, root=None):
        self.commands.append(arguments)
        if arguments[:4] == ["codex", "plugin", "marketplace", "list"]:
            settings = update_plugin.market_settings(update_plugin.read_config(self.home / "config.toml"))
            markets = [{"name": update_plugin.MARKET, "root": str(self.market),
                        "marketplaceSource": {"sourceType": "git", "source": settings["source"]}}] if settings else []
            return json.dumps({"marketplaces": markets if self.market.exists() else []})
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

    def desktop_connection(self, version=None, reload_error=None):
        connection = MagicMock()
        responses = [json.dumps(item) for item in [
            {"method": "notification"}, {"id": 1, "result": {}},
            {"id": 0, "result": {"thread": {"status": {"type": "idle"}}}},
            {"id": 2, "result": {}}, {"id": 3, "result": {}},
            {"id": 4, "result": {"data": [], "nextCursor": "next"}},
            {"id": 5, "result": {"data": [{
                "pluginId": "codex-session-board@session-board-rust",
                "serverInfo": {"name": "codex-session-board", "version": version or self.version},
                "tools": {"board_view": {}}}], "nextCursor": None}},
            {"id": 6, "result": {"content": [], "structuredContent": {"sessions": []}}},
        ]]
        connection.recv.side_effect = responses if reload_error is None else responses[:4] + [reload_error]
        return connection

    def test_full_update_installs_marketplace_file_on_desktop_before_reload(self):
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
            "thread/read", "plugin/install", "config/mcpServer/reload", "mcpServerStatus/list",
            "mcpServerStatus/list", "mcpServer/tool/call"])
        self.assertEqual(calls[3]["params"], {"marketplacePath": str(self.market / ".agents/plugins/marketplace.json"),
                                              "pluginName": "codex-session-board"})
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
        def command(arguments, root=None):
            if arguments == ["git", "rev-parse", "HEAD"] and root == self.market:
                return "wrong-revision"
            return original(arguments, root)
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
                cache = self.home / "plugins/cache" / update_plugin.MARKET / "codex-session-board" / self.version
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
        self.assertEqual([arguments[-1] for arguments in attempts], [self.revision, "baseline"])
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
                shutil.rmtree(old)
                result = original(arguments, **kwargs)
                (cache / "RELEASE.json").write_text("null")
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
        with self.isolated(), \
             patch.object(update_plugin, "cached_package", side_effect=TypeError("unexpected verifier failure")), \
             patch.object(update_plugin, "require_enablement", side_effect=RuntimeError("enablement conflict")), \
             self.assertRaisesRegex(RuntimeError, "unexpected verifier failure.*enablement conflict.*备份保留在"):
            self.update(reload=False)
        backup, = cache.parent.parent.glob(".board-rollback-*")
        self.assertEqual(package_plugin.hashes(backup / "cache" / self.version), expected)
        self.assertEqual(package_plugin.hashes(cache), expected)
        self.assertTrue((backup / "failed-cache" / self.version).is_dir())
        self.assertEqual(json.loads((self.state / "current.json").read_text()), previous)

    def test_rollback_does_not_replace_live_config_after_a_late_write(self):
        config = self.home / "config.toml"
        self.home.mkdir()
        config.write_text(f'[plugins."{update_plugin.PLUGIN_ID}"]\nenabled = false\n')
        read = update_plugin.read_config
        checks = []
        replace = os.replace

        def write_after_read(path):
            snapshot = read(path)
            checks.append(snapshot["plugins"][update_plugin.PLUGIN_ID]["enabled"])
            with path.open("a") as writer:
                writer.write("\n[history]\npersistence = 'none'\n")
            return snapshot

        def prevent_config_replacement(source, destination):
            self.assertNotEqual(destination, config)
            replace(source, destination)

        def check_enablement(*args):
            with patch.object(update_plugin, "read_config", side_effect=write_after_read):
                return check(*args)

        check = update_plugin.require_enablement
        with self.isolated(), patch.object(update_plugin.os, "replace", side_effect=prevent_config_replacement), \
             patch.object(update_plugin, "cached_package", side_effect=ValueError("cache check failed")), \
             patch.object(update_plugin, "require_enablement", side_effect=check_enablement), \
             self.assertRaisesRegex(RuntimeError, "未留下市场注册"):
            self.update(reload=False)
        self.assertEqual(checks, [False])
        self.assertFalse(update_plugin.plugin_enabled(read(config)))
        self.assertEqual(read(config).unwrap()["history"], {"persistence": "none"})

    def test_marketplace_timeout_preserves_registration_from_another_writer(self):
        foreign = {"name": update_plugin.MARKET, "root": str(self.root / "foreign"),
                   "marketplaceSource": {"sourceType": "local", "source": str(self.root / "foreign")}}
        registrations = [None, foreign]
        with self.isolated(), patch.object(update_plugin, "market_registration", side_effect=registrations), \
             patch.object(update_plugin, "add_market", side_effect=subprocess.TimeoutExpired("marketplace add", 120)), \
             patch.object(update_plugin, "remove_market") as remove, \
             self.assertRaisesRegex(RuntimeError, "市场注册冲突.*已保留当前来源.*备份保留在"):
            self.update(reload=False)
        remove.assert_not_called()
        self.assertFalse((self.state / "current.json").exists())

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
             patch.object(verify_plugin_upgrade, "cached_package", side_effect=lambda *args:
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
        config.write_text('# user comment\n' + multiline_history + '[analytics]\nenabled = false\n')
        if initial_enabled is not None:
            if not cached_source:
                shutil.copytree(legacy, old_cache)
            with config.open("a") as settings:
                settings.write(f"\n[{plugin_key}]\nenabled = {str(initial_enabled).lower()}\n")
        initial = update_plugin.read_config(config).unwrap()
        previous = {"name": update_plugin.MARKET, "root": str(legacy),
                    "marketplaceSource": {"sourceType": "local", "source": str(legacy)}}
        registration = previous
        candidate = {"name": update_plugin.MARKET, "root": str(self.market),
                     "marketplaceSource": {"sourceType": "git", "source": "mock-repository"}}

        def add_market(codex, repository, revision=None, checkout=None):
            nonlocal registration
            if checkout is not None and install_failure == "market":
                raise subprocess.TimeoutExpired("marketplace add", 120)
            if checkout is None:
                self.assertTrue(Path(repository).is_dir())
            registration = candidate if checkout is not None else previous
            self.write_market_registration(revision)

        def remove_market(codex):
            nonlocal registration
            registration = None
            self.write_market_registration(None)

        def add_plugin(codex, home=None):
            self.assertIsNotNone(home)
            shutil.rmtree(old_cache, ignore_errors=True)
            private_config = home / "config.toml"
            document = update_plugin.read_config(private_config)
            document.setdefault("plugins", {}).setdefault(update_plugin.PLUGIN_ID, {})["enabled"] = True
            private_config.write_text(update_plugin.tomlkit.dumps(document))
            shutil.copytree(self.source, update_plugin.cache_directory(self.version))

        with patch.object(update_plugin, "market_registration", side_effect=lambda codex: registration), \
             patch.object(update_plugin, "add_market", side_effect=add_market), \
             patch.object(update_plugin, "remove_market", side_effect=remove_market), \
             patch.object(update_plugin, "add_plugin", side_effect=add_plugin) as install, \
             patch.object(update_plugin, "command", return_value=self.revision):
            if install_failure == "cache":
                with patch.object(update_plugin, "cached_package", side_effect=ValueError("cache check failed")):
                    with self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场"):
                        update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
            elif install_failure == "market":
                with self.assertRaisesRegex(RuntimeError, "已恢复上一个插件市场"):
                    update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
                install.assert_not_called()
            else:
                update_plugin.install_market("codex", "mock-repository", self.revision, self.source, self.manifest)
        self.assertIn("# user comment\n" + multiline_history, config.read_text())
        if install_failure:
            self.assertEqual(registration, previous)
            self.assertFalse(update_plugin.cache_directory(self.version).exists())
            self.assertEqual(old_cache.exists(), initial_enabled is not None)
            if old_cache.exists():
                self.assertEqual(package_plugin.hashes(old_cache), legacy_hashes)
            current = update_plugin.read_config(config).unwrap()
            for document in (initial, current):
                for key in ("plugins", "marketplaces"):
                    if not document.get(key):
                        document.pop(key, None)
            self.assertEqual(current, initial)
        else:
            self.assertEqual(registration, candidate)
            self.assertEqual(update_plugin.verify_package(update_plugin.cache_directory(self.version)), self.manifest)
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
