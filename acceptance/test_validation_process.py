import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

from scripts import validation_process


ROOT = Path(__file__).resolve().parents[1]
WORKER = """
import json,os,pathlib,signal,subprocess,sys,time
root=pathlib.Path(sys.argv[1])
mode=sys.argv[2]
descendant=subprocess.Popen([sys.executable,"-c",
    "import pathlib,signal,sys,time;signal.signal(signal.SIGINT,signal.SIG_IGN);"
    "pathlib.Path(sys.argv[1]).touch();time.sleep(60)",str(root/"descendant-ready")])
while not (root/"descendant-ready").exists(): time.sleep(.02)
(root/"ready.json").write_text(json.dumps({"worker":os.getpid(),"descendant":descendant.pid}))
if mode in ("normal","failure"): sys.exit(0 if mode=="normal" else 7)
time.sleep(60)
"""
OWNER = """
import json,os,pathlib,sys
sys.path.insert(0,sys.argv[1])
from scripts.validation_process import execute
root=pathlib.Path(sys.argv[2])
try:
    result=execute("step-1",json.loads(sys.argv[3]),root,root=root,environment=dict(os.environ),
                   timeout=float(sys.argv[4]),run_id="owned-test")
    (root/"result.json").write_text(json.dumps(result))
except KeyboardInterrupt:
    sys.exit(130)
except RuntimeError as error:
    (root/"error.txt").write_text(str(error))
    sys.exit(3)
"""


def await_file(path, process, seconds=12):
    deadline = time.monotonic() + seconds
    while not path.exists():
        if process.poll() is not None or time.monotonic() >= deadline:
            raise AssertionError(f"进程 {process.pid} 未生成 {path.name}，退出码 {process.poll()}")
        time.sleep(0.02)
    return json.loads(path.read_text())


class ValidationProcessTests(unittest.TestCase):
    def test_all_exit_modes_reap_descendants_and_preserve_an_unrelated_process(self):
        unrelated = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(60)"],
                                     start_new_session=True)
        try:
            for mode in ("normal", "failure", "timeout", "interrupt", "hard_kill"):
                with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                    self.exercise(Path(temporary), mode)
                    self.assertIsNone(unrelated.poll())
        finally:
            unrelated.kill()
            unrelated.wait(timeout=5)

    def exercise(self, root, mode):
        command = [sys.executable, "-c", WORKER, str(root), mode]
        with (root / "owner.log").open("w") as log:
            owner = subprocess.Popen(
                [sys.executable, "-c", OWNER, str(ROOT), str(root), json.dumps(command),
                 "1.5" if mode == "timeout" else "30"],
                stdout=log, stderr=log, start_new_session=True)
            try:
                ready = await_file(root / "ready.json", owner)
                self.assertNotEqual(ready["worker"], ready["descendant"])
                if mode == "interrupt":
                    owner.send_signal(signal.SIGINT)
                elif mode == "hard_kill":
                    owner.kill()
                owner.wait(timeout=20)
                path = root / "step-1.lease.json"
                deadline = time.monotonic() + 15
                while True:
                    receipt = json.loads(path.read_text())
                    if receipt["cleanup_confirmed"] or time.monotonic() >= deadline:
                        break
                    time.sleep(0.05)
                self.assertTrue(receipt["cleanup_confirmed"], receipt)
                expected = {"normal": 0, "failure": 0, "timeout": 3,
                            "interrupt": 130, "hard_kill": -signal.SIGKILL}
                self.assertEqual(owner.returncode, expected[mode], (root / "owner.log").read_text())
                if mode in ("normal", "failure"):
                    result = json.loads((root / "result.json").read_text())
                    self.assertEqual(result["returncode"], 0 if mode == "normal" else 7)
                else:
                    self.assertEqual(receipt["reason"], "timeout" if mode == "timeout" else "owner_lost")
                groups = subprocess.check_output(["/bin/ps", "-axo", "pgid=,stat="], text=True)
                self.assertFalse(any(parts[0] == str(receipt["group_pid"]) and not parts[1].startswith("Z")
                                     for row in groups.splitlines() if len(parts := row.split()) == 2))
            finally:
                if owner.poll() is None:
                    owner.kill()
                owner.wait(timeout=5)

    def test_missing_or_unconfirmed_receipts_cannot_prove_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "step-1.request.json").write_text("{}")
            with self.assertRaises(FileNotFoundError):
                validation_process.cleanup_receipts(root, "run")
            (root / "step-1.lease.json").write_text(json.dumps({
                "run_id": "run", "name": "step-1", "cleanup_confirmed": False}))
            with self.assertRaises(RuntimeError):
                validation_process.cleanup_receipts(root, "run")
