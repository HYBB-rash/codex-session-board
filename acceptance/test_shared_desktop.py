from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts import shared_desktop


class SharedDesktopTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir="/private/tmp" if sys.platform == "darwin" else None)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.profile = self.root / "profile"
        self.home.mkdir(mode=0o700)
        self.profile.mkdir(mode=0o700)
        self.report = self.root / "report.json"
        self.backend = self.executable("backend", """
import json,os,pathlib,socket,sys
if sys.argv[1:] == ['--version']:
    print('codex-cli 9.1');sys.exit()
home=pathlib.Path(os.environ['CODEX_HOME'])
(home/'backend.json').write_text(json.dumps(dict(os.environ)))
server=socket.socket(socket.AF_UNIX);server.bind(sys.argv[-1].removeprefix('unix://'))
os.chmod(server.getsockname(),0o600);server.listen()
while True:
    stream,_=server.accept();stream.recv(1);stream.close()
""")

    def executable(self, name, body):
        path = self.root / name
        path.write_text("#!/usr/bin/env python3\n" + body)
        path.chmod(0o700)
        return path

    def launch(self, body):
        app = self.executable("app", body)
        with redirect_stdout(io.StringIO()):
            return shared_desktop.launch(app, self.backend, self.home, self.profile,
                                         self.report, probe=True, timeout=1)

    def stopped_report(self):
        report = json.loads(self.report.read_text())
        self.assertTrue(report["cleanup_confirmed"])
        for field in ("backend_pid", "desktop_pid"):
            if field in report:
                with self.assertRaises(ProcessLookupError):
                    os.kill(report[field], 0)
        return report

    def test_shared_environment_and_peer_identity_are_checked_and_children_are_reaped(self):
        body = """
import json,os,pathlib,time
home=pathlib.Path(os.environ['CODEX_HOME'])
(home/'desktop.json').write_text(json.dumps(dict(os.environ)))
print('currentVersion=9.1 hostId=local',flush=True)
print('outcome=success transportKind=websocket next=connected',flush=True)
time.sleep(30)
"""
        with patch.dict(os.environ, {"CODEX_CLI_PATH": "/wrong/backend",
                                    "CODEX_APP_SERVER_WS_URL": "ws://wrong/rpc",
                                    "OPENAI_API_KEY": "probe-must-not-inherit"}):
            report = self.launch(body)
        backend = json.loads((self.home / "backend.json").read_text())
        desktop = json.loads((self.home / "desktop.json").read_text())
        for environment in (backend, desktop):
            self.assertNotIn("CODEX_CLI_PATH", environment)
            self.assertNotIn("OPENAI_API_KEY", environment)
            self.assertEqual(environment["CODEX_APP_SERVER_WS_URL"],
                             f"ws+unix://localhost{report['endpoint']}:/rpc")
        self.assertEqual(report["endpoint_peer_pid"], report["backend_pid"])
        self.assertEqual(backend["CODEX_SESSION_BOARD_SHARED_AUTHORITY"],
                         desktop["CODEX_SESSION_BOARD_SHARED_AUTHORITY"])
        authority = json.loads(Path(backend["CODEX_SESSION_BOARD_SHARED_AUTHORITY"]).read_text())
        self.assertEqual(authority, report["authority"])
        self.assertEqual(authority["backend"]["pid"], report["backend_pid"])
        self.assertEqual(authority["client"]["pid"], report["desktop_pid"])
        self.assertEqual(authority["launcher"], shared_desktop.process_identity(os.getpid()))
        self.assertTrue(self.stopped_report()["passed"])

    def test_wrong_endpoint_process_is_rejected_before_launching_the_desktop(self):
        with patch.object(shared_desktop, "endpoint_pid", return_value=os.getpid()):
            with self.assertRaisesRegex(RuntimeError, "不属于"):
                self.launch("raise RuntimeError('must not launch')")
        report = self.stopped_report()
        self.assertFalse(report["passed"])
        self.assertNotIn("desktop_pid", report)

    def test_stdio_fallback_or_wrong_version_never_passes_startup(self):
        for output, error in [("stdio_transport_spawned", RuntimeError),
                              ("currentVersion=8 hostId=local", TimeoutError)]:
            with self.assertRaises(error):
                self.launch(f"import time\nprint({output!r},flush=True)\ntime.sleep(30)")
            self.assertFalse(self.stopped_report()["passed"])

    def test_single_instance_handoff_is_a_failure_and_only_owned_backend_is_stopped(self):
        with self.assertRaisesRegex(RuntimeError, "已有客户端"):
            self.launch("pass")
        self.assertFalse(self.stopped_report()["passed"])
