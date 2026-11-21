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

    def test_only_verified_groups_receive_two_threads(self):
        output = self.target("test first ... ok\n")
        for prefix, threads in (
            ("lib::codex_session_board", 2), ("test::native_authority", 2),
            ("test::git_hooks", 2), ("example::project_check", 2),
            ("test::performance", 1), ("test::fixture", 1), ("lib::other", 1),
        ):
            with self.subTest(prefix=prefix), mock.patch.object(
                rust_adapter, "command", return_value=output
            ) as command:
                _, timing = rust_adapter.run_target(prefix, "/binary", {prefix + "::first"})
                self.assertIn(f"--test-threads={threads}", command.call_args.args[0])
                self.assertEqual(timing["threads"], threads)

    def test_selection_does_not_execute_unselected_groups(self):
        targets = {"test::board_io": "/board_io", "example::project_check": "/project_check"}
        selected = "test::board_io::case"
        unselected = "example::project_check::case"
        executed = []

        def binary(arguments, **options):
            if "--list" in arguments:
                return self.target("case: test\n")
            executed.append(arguments[0])
            return self.target("test case ... ok\n")

        with (
            mock.patch.object(rust_adapter, "executables", return_value=targets),
            mock.patch.object(rust_adapter, "python_inventory", return_value=[]),
            mock.patch.object(rust_adapter, "command", side_effect=binary),
        ):
            report = rust_adapter.run(
                {"protocol": 1, "tests": [selected, unselected]}, prefixes=["test::board_io"]
            )
        self.assertEqual(executed, ["/board_io"])
        self.assertEqual(report["selected"], [selected])
        self.assertEqual(report["not_run"], [unselected])
        results = [(row["id"], row["status"]) for row in report["results"]]
        self.assertEqual(results, [(selected, "pass")])
