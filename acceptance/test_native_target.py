import importlib.util
from pathlib import Path
import runpy
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_target_prepare", ROOT / "scripts/native_new/prepare.py")
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


class NativeTargetTests(unittest.TestCase):
    def test_target_navigation_never_changes_operator_project_or_sends_input(self):
        opener = (ROOT / "scripts/native_new/open_target.js").read_text().strip()
        branch = NATIVE.target_dispatch(opener)
        script = """
import assert from "node:assert/strict";
import fs from "node:fs";
const branch = fs.readFileSync(0, "utf8");
const route={kind:"targetNewThread",hostId:"remote:a",projectId:"backend",
  operationId:"019a1111-2222-4333-8444-555555555555"};
const events=[];
const invoke=new Function("t","l","e",`return(async()=>{switch(t.kind){${branch}}})();`);
await invoke(route,async(window,path,state)=>events.push({path,state}),{});
assert.equal(events.length,1);
assert.equal(events[0].path,"/");
assert.deepEqual(events[0].state.nativeTargetDraft,{
  hostId:route.hostId,projectId:route.projectId,operationId:route.operationId});
assert.equal(typeof events[0].state.focusComposerNonce,"number");
"""
        subprocess.run(["node", "--input-type=module", "-e", script],
                       input=branch, text=True, capture_output=True, check=True, timeout=30)

    def test_target_candidate_requires_complete_exact_renderer_profile(self):
        adapter = runpy.run_path(str(ROOT / "scripts/native_new/target_patches.py"))
        for sources in [{}, {path: b"unknown renderer" for path in adapter["SOURCES"]},
                        {path: b"unknown renderer" for path in adapter["FEDORA_SOURCES"]}]:
            with self.subTest(paths=sorted(sources)):
                with self.assertRaisesRegex(ValueError, "Unsupported target draft renderer"):
                    adapter["patch_sources"](sources)
