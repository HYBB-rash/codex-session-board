import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import update_plugin
import reload_plugins


class UpdateDeliveryTests(unittest.TestCase):
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
            root = Path(temporary)
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
