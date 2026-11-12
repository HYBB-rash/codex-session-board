import json
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import tempfile
import sys
import time
import threading
import unittest
import urllib.error

from scripts.dev_environment import configuration
from scripts.dev_environment.processes import Child, reap


@contextmanager
def endpoint(payload, code=200, headers=None):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(self.path)
            body = json.dumps(payload).encode()
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
        from scripts.dev_environment.runtime import fetch
        with endpoint({}, 302, {"Location": "/api/sessions"}) as (url, requests):
            with self.assertRaises(urllib.error.HTTPError):
                fetch(url + "/api/info")
            self.assertEqual(requests, ["/api/info"])
