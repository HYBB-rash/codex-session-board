import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class SmokeCliTests(unittest.TestCase):
    def test_live_write_and_recovery_reject_other_cards_and_concurrent_changes(self):
        source = """
import assert from "node:assert/strict";
import { unchanged, stateWrite } from "./scripts/smoke-guards.mjs";
const owned={id:"own",state:"review",note:"original",manual:true,version:7,inputVersion:3,sectionVersion:"s"};
const body={id:"own",state:"blocked",note:"",version:7,inputVersion:3,sectionVersion:"s"};
const request=payload=>({url:"http://127.0.0.1:18767/api/state",method:"POST",postData:JSON.stringify(payload)});
assert.deepEqual(stateWrite(request(body),owned,"http://127.0.0.1:18767"),body);
unchanged({...owned},owned);
for(const key of ["id","version","inputVersion","sectionVersion"])
  assert.throws(()=>stateWrite(request({...body,[key]:"different"}),owned,"http://127.0.0.1:18767"));
for(const key of ["state","note","manual","version","inputVersion","sectionVersion"])
  assert.throws(()=>unchanged({...owned,[key]:"external"},owned));
assert.throws(()=>stateWrite({...request(body),url:"http://example.com/api/state"},owned,"http://127.0.0.1:18767"));
assert.throws(()=>unchanged(null,owned));
"""
        subprocess.run(["node", "--input-type=module", "-e", source], cwd=ROOT, check=True, timeout=10)

    def test_unverified_or_nonlocal_receipts_stop_before_opening_a_browser(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index, proof in enumerate((
                    {"passed": False, "phase": "awaiting_desktop_reload"},
                    {"passed": True, "phase": "desktop_loaded", "url": "http://example.com"})):
                with self.subTest(proof=proof):
                    receipt = root / f"receipt-{index}.json"
                    receipt.write_text(json.dumps(proof))
                    output = root / f"result-{index}"
                    result = subprocess.run([
                        "node", str(ROOT / "scripts/smoke_plugin.mjs"),
                        "--receipt", str(receipt), "--session-id", "a" * 36, "--output", str(output),
                    ], capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    report = json.loads((output / "SMOKE.json").read_text())
                    self.assertFalse(report["passed"])
                    self.assertEqual(report["cases"], [])
                    self.assertNotIn("cleanup", report)
