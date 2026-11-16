import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class NativeNewTests(unittest.TestCase):
    def test_official_entry_has_no_client_rewriting_tools_or_capability_gate(self):
        for name in ("prepare.py", "prepare_app.py"):
            self.assertFalse((ROOT / "scripts/native_new" / name).exists())
        source = (ROOT / "src/native_new.rs").read_text()
        self.assertNotIn("app.asar", source)
        self.assertNotIn("native_new_capability", source)
        self.assertIn("codex://settings/connections/ssh/add", source)
        self.assertNotIn("codex://threads/new-remote", source)

    def test_native_navigation_and_assignment_do_not_create_or_send_input(self):
        for name in ("src/native_new.rs", "src/app/project_assignment.rs"):
            source = (ROOT / name).read_text()
            for method in ('"thread/start"', '"turn/start"', '"thread/resume"'):
                self.assertNotIn(method, source)
        source = (ROOT / "src/app/project_assignment.rs").read_text()
        self.assertIn('"thread/metadata/update"', source)
        self.assertIn('"thread/read"', source)
        self.assertIn("Role::ProjectWriter", source)
