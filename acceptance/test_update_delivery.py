import json
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

import websocket

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import update_plugin
import reload_plugins


class UpdateDeliveryTests(unittest.TestCase):
    def test_cache_provenance_requires_the_native_git_revision_even_without_a_cache_marker(self):
        markets = json.dumps({"marketplaces": [{
            "name": update_plugin.MARKET, "root": "/native-snapshot",
            "marketplaceSource": {"sourceType": "git"},
        }]})
        with tempfile.TemporaryDirectory() as temporary:
            with patch.dict(update_plugin.os.environ, {"CODEX_HOME": temporary}):
                with patch.object(update_plugin, "verify_package", return_value={"verified": True}) as verify:
                    with patch.object(update_plugin, "command", side_effect=[markets, "expected"]):
                        self.assertEqual(update_plugin.cached_package("codex", "new", "expected"),
                                         {"verified": True})
                    verify.assert_called_once()
                    verify.reset_mock()
                    with patch.object(update_plugin, "command", side_effect=[markets, "different"]):
                        with self.assertRaisesRegex(ValueError, "Git revision differs"):
                            update_plugin.cached_package("codex", "new", "expected")
                    verify.assert_not_called()

    def test_native_add_can_only_clone_the_verified_local_snapshot(self):
        revision = "a" * 40
        canonical = "https://github.com/example/repo.git"
        ssh = "git@github.com:example/repo.git"
        cases = [
            ("https://github.com/example/repo", canonical),
            ("https://github.com/example/repo/", canonical),
            (canonical, canonical),
            (f"{canonical}/", canonical),
            (ssh, ssh),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary).resolve()
            git = subprocess.run
            for repository, source in cases:
                with self.subTest(repository=repository), patch.object(update_plugin.subprocess, "run") as run:
                    update_plugin.add_market("codex", repository, revision, checkout)
                    run.assert_called_once()
                    self.assertEqual(run.call_args.args[0],
                                     ["codex", "plugin", "marketplace", "add", source, "--json", "--ref", revision])
                    environment = run.call_args.kwargs["env"]
                    self.assertEqual(environment["GIT_ALLOW_PROTOCOL"], "file")
                    resolved = git(["git", "ls-remote", "--get-url", source], cwd=checkout,
                                   env=environment, check=True, capture_output=True, text=True)
                    self.assertEqual(resolved.stdout.strip(), checkout.as_uri())

    def test_reload_targets_the_installed_mcp_without_global_skill_discovery(self):
        server = {"pluginId": "codex-session-board@session-board-rust",
                  "serverInfo": {"name": "codex-session-board", "version": "new"},
                  "tools": {"board_view": {}}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            path = root / "desktop.sock"
            marketplace = root / "marketplace.json"
            marketplace.write_text(json.dumps({"name": update_plugin.MARKET}))
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(str(path))
                path.chmod(0o600)
                for install in (False, True):
                    with self.subTest(install=install):
                        connection = MagicMock()
                        responses = [{"id": 1, "result": {}},
                                     {"id": 0, "result": {"thread": {"status": {"type": "idle"}}}}]
                        if install:
                            responses.append({"id": 2, "result": {}})
                        responses.extend([
                            {"id": 3, "result": {}},
                            {"id": 4, "result": {"data": [server], "nextCursor": None}},
                            {"id": 5, "result": {"content": [], "structuredContent": {"sessions": []}}},
                        ])
                        connection.recv.side_effect = [json.dumps(item) for item in responses]
                        with patch.object(websocket, "create_connection", return_value=connection), \
                             patch.dict(reload_plugins.os.environ, {
                                 "CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET": str(path),
                                 "CODEX_THREAD_ID": "fixture-thread"}), \
                             patch.object(reload_plugins.socket, "socket") as transport, \
                             patch.object(update_plugin.subprocess, "check_output",
                                          side_effect=AssertionError("No daemon lookup")):
                            result = reload_plugins.reload_plugins(
                                "codex", root, "new", marketplace if install else None)
                        self.assertTrue(result["desktop_loaded"])
                        self.assertEqual(result["phase"], "desktop_loaded")
                        self.assertEqual(result["socket"], str(path))
                        self.assertEqual(result["mcp"]["serverInfo"]["version"], "new")
                        calls = [json.loads(call.args[0]) for call in connection.send.call_args_list]
                        methods = ["initialize", "initialized", "thread/read"]
                        if install:
                            methods.append("plugin/install")
                            self.assertEqual(calls[3]["params"], {
                                "marketplacePath": str(marketplace), "pluginName": "codex-session-board"})
                        methods.extend(["config/mcpServer/reload", "mcpServerStatus/list", "mcpServer/tool/call"])
                        self.assertEqual([call["method"] for call in calls], methods)
                        self.assertEqual(calls[-2]["params"]["threadId"], "fixture-thread")
                        self.assertEqual(calls[-1]["params"], {"threadId": "fixture-thread",
                            "server": "session_board", "tool": "board_snapshot", "arguments": {}})
                        transport.return_value.__enter__.return_value.connect.assert_called_once_with(str(path))
                        connection.close.assert_called_once()
            with patch.dict(reload_plugins.os.environ, {"CODEX_THREAD_ID": ""}), \
                    patch.object(reload_plugins, "desktop_socket", return_value=path), \
                    patch.object(reload_plugins.socket, "socket") as transport:
                self.assertFalse(reload_plugins.reload_plugins("codex", root, "new")["desktop_loaded"])
                transport.assert_not_called()

    def test_pending_candidate_never_stops_the_installed_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(update_plugin.subprocess, "run"), patch.object(update_plugin, "target", return_value="linux"):
                with patch.object(update_plugin, "verify_package", return_value={"target":"linux", "source_commit":"old"}):
                    with patch.object(update_plugin, "command") as command:
                        with self.assertRaisesRegex(ValueError, "not yet available"):
                            update_plugin.update("repository", "candidate", None,
                                                 root / "state", "codex", expected_source="new")
                        command.assert_not_called()

    def test_reload_does_not_accept_an_old_mcp_after_cache_update(self):
        server = {"pluginId": "codex-session-board@session-board-rust",
                  "serverInfo": {"name": "codex-session-board", "version": "old"},
                  "tools": {"board_view": {}}}
        def response(identifier, method, params):
            return {"data": [server], "nextCursor": None}
        with self.assertRaisesRegex(RuntimeError, "version and tools"):
            reload_plugins.verify_loaded(response, "new")
        server["serverInfo"]["version"] = "new"
        self.assertEqual(reload_plugins.verify_loaded(response, "new")["pluginId"], server["pluginId"])
        with self.assertRaisesRegex(RuntimeError, "snapshot"):
            reload_plugins.verify_loaded(response, "new", "fixture-thread")
        connection = MagicMock()
        connection.recv.side_effect = [json.dumps(item) for item in [
            {"id": 1, "result": {}},
            {"id": 0, "result": {"thread": {"status": {"type": "notLoaded"}}}},
        ]]
        with patch.dict(reload_plugins.os.environ, {"CODEX_THREAD_ID": "fixture-thread"}), \
                patch.object(reload_plugins, "desktop_socket", return_value=Path("/fixture.sock")), \
                patch.object(reload_plugins.socket, "socket"), \
                patch.object(websocket, "create_connection", return_value=connection):
            with self.assertRaisesRegex(RuntimeError, "not loaded"):
                reload_plugins.reload_plugins("codex", Path("/fixture"), "new")
        methods = {json.loads(call.args[0])["method"] for call in connection.send.call_args_list}
        self.assertIn("thread/read", methods)
        self.assertLessEqual(methods, {"initialize", "initialized", "thread/read", "mcpServerStatus/list"})

    def test_wrong_architecture_is_rejected_before_stopping_or_installing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(update_plugin.subprocess, "run"):
                with patch.object(update_plugin, "target", return_value="linux"):
                    with patch.object(update_plugin, "verify_package", return_value={"target": "macos"}):
                        with patch.object(update_plugin, "install_market") as install:
                            with patch.object(update_plugin, "command") as command:
                                with self.assertRaisesRegex(ValueError, "different operating"):
                                    update_plugin.update("test-repository", "candidate", None,
                                                         root / "state", "codex")
                                install.assert_not_called()
                                command.assert_not_called()
