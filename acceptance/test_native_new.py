import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_new", ROOT / "scripts/native_new/prepare.py")
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


class NativeNewTests(unittest.TestCase):
    def test_native_draft_routes_and_navigation_never_send_input(self):
        subprocess.run(["node", "scripts/check-native-new.mjs"], cwd=ROOT, check=True,
                       capture_output=True, timeout=30)

    def test_candidate_archive_preserves_other_files_and_rejects_unknown_clients(self):
        sources = {
            "/.vite/build/routes.js": b'function parse(t){switch(t.host){case"threads":{let e=L(t);return e}}}',
            "/.vite/build/main.js": b'async function show(){switch(t.kind){case"newThread":{let s=1;return s}}}',
            "/untouched.txt": b"preserve original content",
        }
        tree = {"files": {}}
        raw = bytearray()
        for name, data in sources.items():
            current = tree
            parts = name.strip("/").split("/")
            for part in parts[:-1]:
                current = current["files"].setdefault(part, {"files": {}})
            current["files"][parts[-1]] = {"offset": str(len(raw)), "size": len(data)}
            raw.extend(data)
        archive = NATIVE.encode_archive(bytes(raw), 0, tree, {})
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "original.asar"
            app = Path(directory) / "Candidate.app"
            target = app / "Contents/Resources/app.asar"
            target.parent.mkdir(parents=True)
            source.write_bytes(archive)
            supported = {name: hashlib.sha256(data).hexdigest()
                         for name, data in sources.items() if name.endswith(".js")}
            with patch.object(NATIVE, "SUPPORTED_SOURCES", supported):
                capability = NATIVE.prepare(source, target, ["open", "-a", str(app)])
            candidate, base, patched = NATIVE.read_archive(target)
            self.assertEqual(capability["sha256"], hashlib.sha256(candidate).hexdigest())
            self.assertEqual(source.read_bytes(), archive)
            for name, entry in NATIVE.files(patched):
                start = base + int(entry["offset"])
                data = candidate[start:start + entry["size"]]
                if name == "/untouched.txt":
                    self.assertEqual(data, sources[name])
                else:
                    self.assertEqual(entry["integrity"]["hash"], hashlib.sha256(data).hexdigest())
            with self.assertRaises(ValueError):
                NATIVE.prepare(source, target, ["open"])
            unrelated = target.with_name("unrelated.asar")
            with self.assertRaises(ValueError):
                NATIVE.prepare(source, unrelated, ["open", "-a", str(app)])
            self.assertFalse(unrelated.exists())
            with self.assertRaises(ValueError):
                NATIVE.patches({"files": {}}, b"", 0)
