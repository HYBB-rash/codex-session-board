import json
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
import socket
from pathlib import Path
import tempfile
import sys
import time
import threading
import unittest

from scripts.dev_environment import configuration
from scripts.dev_environment.processes import Child, reap


@contextmanager
def endpoint(payload, code=200, headers=None):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def do_GET(self):
            requests.append(self.path)
            body = json.dumps(payload(self.path) if callable(payload) else payload).encode()
            self.send_response(code)
            self.send_header("Content-Length", str(len(body)))
            for name, value in (headers or {}).items():
                self.send_header(name, value)
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05})
        thread.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}", requests
        finally:
            server.shutdown()
            thread.join(timeout=3)


class DevEnvironmentTests(unittest.TestCase):
    def test_native_health_cannot_publish_readonly_cards_as_ready(self):
        from scripts.dev_environment.runtime import probe
        identity = {"pid": os.getpid(), "build": "expected", "dataDir": "/task/data"}
        row = {"id": "fixture", "readOnly": True, "sectionVersion": None}
        snapshot = {
            "health": {"scanComplete": True, "runtimeVerified": True, "nativeVerified": True},
            "projectCatalog": {"loaded": True}, "sessions": [row],
        }
        with endpoint(lambda path: identity if path == "/api/info" else snapshot) as (url, _):
            record = {"url": url, "service_pid": os.getpid(), "program_sha256": "expected", "root": "/task"}
            with self.assertRaisesRegex(RuntimeError, "可写会话"):
                probe(record)
            row.update(sectionVersion="native:fixture")
            with self.assertRaisesRegex(RuntimeError, "可写会话"):
                probe(record)
            row.update(readOnly=False, sectionVersion=None)
            with self.assertRaisesRegex(RuntimeError, "可写会话"):
                probe(record)
            row.update(sectionVersion="native:fixture")
            probe(record)
            row.update(remote=True)
            with self.assertRaisesRegex(RuntimeError, "可写会话"):
                probe(record)
            snapshot["sessions"] = []
            with self.assertRaisesRegex(RuntimeError, "可写会话"):
                probe(record)

    def test_fixture_cleanup_includes_stale_mcp_socket(self):
        from scripts.dev_environment.runtime import clear_fixture_sockets
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ("desktop/rpc.sock", "sidebar/rpc.sock", "mcp.sock"):
                path = root / relative
                path.parent.mkdir(exist_ok=True)
                with socket.socket(socket.AF_UNIX) as server:
                    server.bind(str(path))
                self.assertTrue(path.exists())
            clear_fixture_sockets(root)
            self.assertFalse(any(root.rglob("*.sock")))

    def test_fixture_cleanup_rejects_active_mcp_listener(self):
        from scripts.dev_environment.runtime import clear_fixture_sockets
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "mcp.sock"
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(path))
                server.listen(1)
                with self.assertRaisesRegex(ValueError, "已有监听者"):
                    clear_fixture_sockets(root)
                self.assertTrue(path.exists())

    def test_fixture_cleanup_preserves_non_socket_and_symlink(self):
        from scripts.dev_environment.runtime import clear_fixture_sockets
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path, target = root / "mcp.sock", root / "target"
            target.write_text("unchanged")
            path.write_text("not a socket")
            with self.assertRaisesRegex(ValueError, "未知文件"):
                clear_fixture_sockets(root)
            self.assertEqual(path.read_text(), "not a socket")
            path.unlink()
            path.symlink_to(target)
            with self.assertRaisesRegex(ValueError, "未知文件"):
                clear_fixture_sockets(root)
            with self.assertRaisesRegex(ValueError, "越界"):
                configuration.boundary(root)
            self.assertTrue(path.is_symlink())
            self.assertEqual(target.read_text(), "unchanged")

    def test_boundary_accepts_a_system_parent_directory_alias(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            target = parent / "canonical"
            target.mkdir()
            (parent / "alias").symlink_to(target.resolve(), target_is_directory=True)
            root = parent / "alias/task"
            root.mkdir(mode=0o700)
            configuration.boundary(root)

    def test_private_directories_reject_symlinks_and_shared_permissions(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            shared = parent / "shared"
            shared.mkdir(mode=0o755)
            shared.chmod(0o755)
            linked = parent / "linked"
            linked.symlink_to(shared, target_is_directory=True)
            for directory in (shared, linked):
                with self.assertRaises(ValueError):
                    configuration.private_directory(directory)

    def test_child_environment_does_not_inherit_desktop_or_credentials(self):
        with tempfile.TemporaryDirectory() as temporary:
            before = dict(os.environ)
            environment = configuration.child_environment(Path(temporary))
            self.assertEqual(dict(os.environ), before)
            self.assertNotIn("SSH_AUTH_SOCK", environment)
            self.assertTrue(environment["CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET"].startswith(temporary))
            for name in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"):
                self.assertTrue(environment[name].startswith(temporary))

    def test_production_data_path_is_rejected_even_with_a_matching_checksum(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "config.json"
            config.write_text(json.dumps({"data_dir": "/production"}))
            manifest = {"source": str(root), "fixture": "/fixture",
                        "files": {"config.json": configuration.digest(config)}}
            with self.assertRaisesRegex(ValueError, "边界"):
                configuration.configuration(root, root, manifest)

    def test_prepare_rejects_a_config_symlink_before_invoking_the_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            production = parent / "production.json"
            production.write_text("unchanged")
            root = parent / "task"
            root.mkdir(mode=0o700)
            (root / "config.json").symlink_to(production)
            with self.assertRaisesRegex(ValueError, "越界"):
                configuration.configure(root, root, {})
            self.assertEqual(production.read_text(), "unchanged")

    def test_owned_group_cleanup_terminates_a_child_that_ignores_interrupt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            child = Child(root, "test", [sys.executable, "-c",
                          "import signal,sys,time;signal.signal(signal.SIGINT,signal.SIG_IGN);"
                          "open(sys.argv[1],'w').close();time.sleep(300)", str(root / "ready")],
                          configuration.child_environment(root))
            try:
                deadline = time.monotonic() + 5
                while not (root / "ready").exists() and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue((root / "ready").exists())
                self.assertIsNotNone(child.pid)
            finally:
                reap([child])
            self.assertEqual(child.guard.returncode, -9)

    def test_foreign_identity_is_rejected_before_reading_sessions(self):
        from scripts.dev_environment.runtime import probe
        with endpoint({"pid": -1, "build": "other", "dataDir": "/formal"}) as (url, requests):
            with self.assertRaisesRegex(RuntimeError, "身份"):
                probe({"url": url, "service_pid": os.getpid(), "program_sha256": "expected", "root": "/task"})
            self.assertEqual(requests, ["/api/info"])

    def test_http_redirect_cannot_leave_the_expected_endpoint(self):
        from scripts.dev_environment.runtime import probe
        with endpoint({}, 302, {"Location": "/api/sessions"}) as (url, requests):
            with self.assertRaisesRegex(RuntimeError, "HTTP 302"):
                probe({"url": url})
            self.assertEqual(requests, ["/api/info"])

    def test_closed_identity_connection_cannot_reconnect_to_read_sessions(self):
        from scripts.dev_environment.runtime import probe
        identity = {"pid": os.getpid(), "build": "expected", "dataDir": "/task/data"}
        with endpoint(identity, headers={"Connection": "close"}) as (url, requests):
            with self.assertRaisesRegex(RuntimeError, "连接已关闭"):
                probe({"url": url, "service_pid": os.getpid(), "program_sha256": "expected", "root": "/task"})
            self.assertEqual(requests, ["/api/info"])

    def test_status_reads_final_receipt_after_controller_disconnects(self):
        from scripts.dev_environment.runtime import status
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            configuration.write_record(root / "runtime.json",
                                       {"phase": "running", "_token": "test", "cleanup_confirmed": False})
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(root / "supervisor.sock"))
                os.chmod(root / "supervisor.sock", 0o600)
                server.listen(1)
                server.settimeout(5)

                def finish():
                    connection, _ = server.accept()
                    with connection:
                        connection.makefile("r").readline()
                        configuration.write_record(root / "runtime.json",
                                                   {"phase": "failed", "_token": "test", "cleanup_confirmed": True})

                thread = threading.Thread(target=finish)
                thread.start()
                try:
                    result = status(root)
                    self.assertEqual(result["phase"], "failed")
                    self.assertTrue(result["cleanup_confirmed"])
                    self.assertNotIn("_token", result)
                finally:
                    thread.join(timeout=5)
