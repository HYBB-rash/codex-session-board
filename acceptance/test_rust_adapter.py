import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from scripts import rust_adapter


class RustAdapterTests(unittest.TestCase):
    def target(self, output, returncode=0):
        return subprocess.CompletedProcess([], returncode, output, "")

    def test_target_rejects_missing_duplicate_unexpected_and_false_success(self):
        expected = {"test::fixture::first"}
        outputs = (
            self.target(""),
            self.target("test first ... ok\ntest first ... ok\n"),
            self.target("test other ... ok\n"),
            self.target("test first ... ok\n", 1),
        )
        for output in outputs:
            with self.subTest(output=output), mock.patch.object(
                rust_adapter, "command", return_value=output
            ):
                with self.assertRaises(ValueError):
                    rust_adapter.run_target("test::fixture", "/binary", expected)

    def test_target_preserves_failed_and_ignored_outcomes(self):
        output = self.target("test first ... FAILED\ntest second ... ignored\n", 1)
        with mock.patch.object(rust_adapter, "command", return_value=output):
            records, timing = rust_adapter.run_target(
                "test::fixture", "/binary", {"test::fixture::first", "test::fixture::second"}
            )
        self.assertEqual([record["status"] for record in records.values()], ["fail", "skip"])
        self.assertEqual(timing["returncode"], 1)

    def test_only_isolated_tools_receive_multiple_threads(self):
        output = self.target("test first ... ok\n")
        for prefix, threads in (("test::git_hooks", 2), ("example::project_check", 2),
                                ("test::native_authority", 1)):
            with self.subTest(prefix=prefix), mock.patch.object(
                rust_adapter, "command", return_value=output
            ) as command:
                rust_adapter.run_target(prefix, "/binary", {prefix + "::first"})
                self.assertIn(f"--test-threads={threads}", command.call_args.args[0])

    def test_selection_does_not_execute_unselected_groups(self):
        names = {"test::first::case": ("test::first", "case"),
                 "test::second::case": ("test::second", "case")}
        with mock.patch.object(rust_adapter, "run_target", return_value=({}, {})) as run:
            rust_adapter.run_rust(
                {"test::first": "/first", "test::second": "/second"},
                names, {"test::first::case"},
            )
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.args[0], "test::first")


class RustAdapterEntryTests(unittest.TestCase):
    def fixture(self, root):
        acceptance = root / "acceptance"
        acceptance.mkdir()
        (acceptance / "__init__.py").touch()
        shutil.copy2(Path(__file__).with_name("unittest_adapter.py"),
                     acceptance / "unittest_adapter.py")
        executable = root / "rust-test"
        executable.write_text(
            f"#!{sys.executable}\nimport sys\n"
            "print('native: test' if '--list' in sys.argv else 'test native ... ok')\n")
        executable.chmod(0o700)
        artifact = {"executable": str(executable), "profile": {"test": True},
                    "target": {"kind": ["test"], "name": "fixture"}}
        cargo = root / "cargo"
        cargo.write_text(f"#!{sys.executable}\nprint({json.dumps(artifact)!r})\n")
        cargo.chmod(0o700)
        environment = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"])
        environment.pop("PYTHONPATH", None)
        return environment

    def test_script_and_module_entries_preserve_target_outcomes_without_pythonpath(self):
        checker = Path(__file__).resolve().parents[1]
        entries = {
            "script": [sys.executable, str(checker / "scripts/rust_adapter.py")],
            "module": [
                sys.executable, "-c",
                "import json,sys\nsys.path.insert(0,sys.argv[1])\n"
                "from scripts import rust_adapter\n"
                "print(json.dumps(rust_adapter.run(json.load(sys.stdin))))\n",
                str(checker),
            ],
        }
        native = "test::fixture::native"
        target = "acceptance.test_target.TargetTests.test_target"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            environment = self.fixture(root)
            for status, assertion in (("pass", "self.assertEqual(2 + 2, 4)"),
                                      ("fail", "self.fail('target failure preserved')")):
                (root / "acceptance/test_target.py").write_text(
                    "import unittest\nclass TargetTests(unittest.TestCase):\n"
                    f" def test_target(self):\n  {assertion}\n")
                for name, arguments in entries.items():
                    with self.subTest(entry=name, status=status):
                        process = subprocess.run(
                            arguments, cwd=root, env=environment,
                            input=json.dumps({"protocol": 1, "tests": [native, target]}),
                            text=True, capture_output=True, timeout=30)
                        self.assertEqual(process.returncode, 0, process.stderr)
                        report = json.loads(process.stdout)
                        self.assertEqual(report["protocol"], 1)
                        self.assertEqual(set(report["inventory"]), {native, target})
                        self.assertEqual(set(report["selected"]), {native, target})
                        self.assertEqual(report["not_run"], [])
                        self.assertEqual(len(report["results"]), 2)
                        records = {item["id"]: item for item in report["results"]}
                        self.assertEqual(
                            {identifier: item["status"] for identifier, item in records.items()},
                            {native: "pass", target: status})
                        if status == "fail":
                            self.assertIn("target failure preserved", records[target]["detail"])
