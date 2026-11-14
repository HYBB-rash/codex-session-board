import copy
import json
from pathlib import Path
import socket
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_package_startup


class PackageStartupTests(unittest.TestCase):
    runtime = {"app": "codex-session-board", "version": "1", "build": "binary",
               "pid": 25, "configId": "configuration", "dataDir": "private",
               "authority": {"pid": 9, "started": 123}}

    def replies(self):
        return [
            {"id": 1, "result": {"serverInfo": {"name": "codex-session-board", "version": "1"}}},
            {"id": 2, "result": {"contents": [
                {"mimeType": "text/html;profile=mcp-app", "text": "<html>board</html>"}]}},
            {"id": 3, "result": {"structuredContent": {
                "sessions": [{"id": "fixture"}], "runtime": copy.deepcopy(self.runtime)}}},
        ]

    def exchange(self, replies, *, exit_code=0, parent=9):
        with tempfile.TemporaryDirectory(prefix="csb-mcp-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            (root / "mcp-child.json").write_text(json.dumps({
                "pid": 26, "parent": parent, "exit": exit_code}))
            requests, failures = [], []
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(root / "mcp.sock"))
                server.listen()

                def answer():
                    try:
                        stream, _ = server.accept()
                        with stream, stream.makefile("r") as incoming:
                            requests.extend(json.loads(line) for line in incoming)
                            stream.sendall("".join(json.dumps(reply) + "\n" for reply in replies).encode())
                    except BaseException as error:
                        failures.append(error)

                worker = threading.Thread(target=answer)
                worker.start()
                try:
                    return verify_package_startup.mcp(root, "1", self.runtime)
                finally:
                    worker.join(timeout=3)
                    self.assertFalse(worker.is_alive())
                    self.assertFalse(failures)
                    self.assertEqual([request.get("id") for request in requests], [1, None, 2, 3])

    def test_fixture_channel_preserves_all_mcp_startup_requests_and_runtime_identity(self):
        self.assertEqual(self.exchange(self.replies()),
                         {"name": "codex-session-board", "version": "1"})

    def test_mcp_failure_wrong_backend_or_different_service_cannot_pass(self):
        for arguments in ({"exit_code": 1}, {"parent": 10}):
            with self.subTest(arguments=arguments), self.assertRaisesRegex(ValueError, "unsuccessfully|backend"):
                self.exchange(self.replies(), **arguments)
        for field, value in (("app", "other"), ("version", "other"), ("build", "other"),
                             ("pid", 99), ("configId", "other"), ("dataDir", "other"),
                             ("authority", {"pid": 9, "started": 124})):
            replies = self.replies()
            replies[2]["result"]["structuredContent"]["runtime"][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "reuse"):
                self.exchange(replies)

    def test_missing_replies_errors_wrong_version_resource_or_empty_snapshot_fail(self):
        cases = [self.replies()[:-1], [{"id": 1, "error": {"message": "failed"}}]]
        for reply_index, field, value in ((0, "serverInfo", {"version": "other"}),
                                          (1, "contents", [{"mimeType": "text/plain", "text": "<html>"}]),
                                          (2, "structuredContent", {"sessions": []})):
            replies = self.replies()
            replies[reply_index]["result"][field] = value
            cases.append(replies)
        for replies in cases:
            with self.subTest(replies=replies), self.assertRaises(ValueError):
                self.exchange(replies)
