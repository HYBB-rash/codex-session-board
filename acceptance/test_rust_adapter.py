import subprocess
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
