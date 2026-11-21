import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class NativeNewTests(unittest.TestCase):
    def test_official_entry_has_no_client_rewriting_tools_or_capability_gate(self):
        for name in ("prepare.py", "prepare_app.py", "target_patches.py", "target.js",
                     "open_target.js", "verify_sources.py", "route.js", "open.js"):
            self.assertFalse((ROOT / "scripts/native_new" / name).exists())
        source = (ROOT / "src/native_new.rs").read_text()
        self.assertNotIn("app.asar", source)
        self.assertNotIn("native_new_capability", source)
        self.assertIn("codex://settings/connections/ssh/add", source)
        self.assertNotIn("codex://threads/new-remote", source)
        self.assertNotIn("codex://threads/new-target", source)

    def test_native_navigation_and_assignment_do_not_create_or_send_input(self):
        result = subprocess.run(["node", "scripts/check-native-new.mjs"], cwd=ROOT,
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
