import json
import sqlite3
import socket
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

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

    def test_install_uses_the_verified_git_snapshot_without_a_second_marketplace_refresh(self):
        revision = "a" * 40
        with patch.object(update_plugin, "command", return_value=json.dumps({
            "marketplaces": [{"name": update_plugin.MARKET}, {"name": "unrelated"}],
        })):
            with patch.object(update_plugin.subprocess, "run") as run:
                update_plugin.install_market("codex", "repository", revision)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands, [
            ["codex", "plugin", "marketplace", "remove", update_plugin.MARKET],
            ["codex", "plugin", "marketplace", "add", "repository", "--ref", revision, "--json"],
            ["codex", "plugin", "add", "codex-session-board@" + update_plugin.MARKET, "--json"],
        ])

    def test_reload_targets_the_installed_mcp_without_global_skill_discovery(self):
        server = {"pluginId": "codex-session-board@session-board-rust",
                  "serverInfo": {"name": "codex-session-board", "version": "new"},
                  "tools": {"board_view": {}}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            path = root / "control.sock"
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(str(path))
                path.chmod(0o600)
                connection = MagicMock()
                connection.recv.side_effect = [
                    json.dumps({"id": 1, "result": {}}),
                    json.dumps({"id": 3, "result": {}}),
                    json.dumps({"id": 4, "result": {"data": [server], "nextCursor": None}}),
                ]
                module = SimpleNamespace(create_connection=MagicMock(return_value=connection))
                with patch.dict(sys.modules, {"websocket": module}):
                    with patch.object(reload_plugins.socket, "socket"):
                        with patch.object(reload_plugins.subprocess, "check_output",
                                          return_value=json.dumps({"socketPath": str(path)})):
                            result = reload_plugins.reload_plugins("codex", root, "new")
                self.assertEqual(result["serverInfo"]["version"], "new")
                self.assertEqual(
                    [json.loads(call.args[0])["method"] for call in connection.send.call_args_list],
                    ["initialize", "initialized", "config/mcpServer/reload", "mcpServerStatus/list"],
                )
                connection.close.assert_called_once()

    def test_pending_candidate_never_stops_the_installed_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(update_plugin.subprocess, "run"), patch.object(update_plugin, "target", return_value="linux"):
                with patch.object(update_plugin, "verify_package", return_value={"target":"linux", "source_commit":"old"}):
                    with patch.object(update_plugin, "command") as command:
                        with self.assertRaisesRegex(ValueError, "not yet available"):
                            update_plugin.update("repository", "candidate", None, root / "config",
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

    def test_backup_preserves_committed_wal_notes_and_all_host_databases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            data = root / "runtime"
            data.mkdir()
            remote = data / "remote-hosts/mac"
            remote.mkdir(parents=True)
            config = root / "config.json"
            original_config = json.dumps({"data_dir": "runtime", "port": 18767})
            config.write_text(original_config)
            connections = []
            try:
                for directory in (data, remote):
                    connection = sqlite3.connect(directory / "status.sqlite3")
                    connections.append(connection)
                    connection.execute("PRAGMA journal_mode=WAL")
                    connection.execute("CREATE TABLE sessions(id TEXT, note TEXT)")
                    connection.execute("INSERT INTO sessions VALUES('same-id','retain this note')")
                    connection.commit()
                (data / "private-key").write_text("never copy unrelated files")
                backup = root / "backup"
                self.assertEqual(update_plugin.backup(config, backup), data)
                for relative in ("status.sqlite3", "remote-hosts/mac/status.sqlite3"):
                    with sqlite3.connect(backup / relative) as copied:
                        self.assertEqual(copied.execute("SELECT * FROM sessions").fetchall(),
                                         [("same-id", "retain this note")])
                self.assertFalse((backup / "private-key").exists())
                self.assertEqual(config.read_text(), original_config)
                self.assertEqual((backup / "config.json").read_text(), original_config)
            finally:
                for connection in connections:
                    connection.close()

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
                                                         root / "config.json", root / "state", "codex")
                                install.assert_not_called()
                                command.assert_not_called()
