import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

from scripts import pr_delivery, pr_plan, rust_adapter


SOURCE = {"commit": "a" * 40, "tree": "b" * 40, "fingerprint": "c" * 64}
INVENTORY = {"lib::application::case", "test::git_hooks::case", "acceptance.case"}


def report(strategy):
    chosen = sorted(INVENTORY - {"test::git_hooks::case"})
    return {"protocol": 1, "scope": "pr_tests", "source": SOURCE, "plan": strategy,
            "passed": True, "release_ready": False, "inventory": sorted(INVENTORY),
            "selected": chosen, "not_run": ["test::git_hooks::case"],
            "results": [{"id": name, "status": "pass"} for name in chosen]}


class PRPlanTests(unittest.TestCase):
    def strategy(self, paths, additions=()):
        with mock.patch.object(pr_plan, "git", return_value="fixed-base"), \
                mock.patch.object(pr_plan, "changed", return_value=("merge-base", paths)):
            return pr_plan.plan(Path("/project"), "moving-reference", additions)

    def test_connection_changes_require_package_and_lifecycle_checks(self):
        strategy = self.strategy(["src/rpc/sidebar.rs"])
        self.assertEqual(strategy["profile"], "business")
        self.assertEqual(strategy["risks"], ["connection", "lifecycle", "package"])
        self.assertTrue(strategy["package_required"])
        self.assertEqual(strategy["base"], "fixed-base")

    def test_ui_records_and_unknown_paths_have_explicit_profiles(self):
        for paths, profile in ((["index.html"], "ui"),
                               (["context/tasks/completed/result.md"], "records"),
                               (["new-build-input"], "full"),
                               (["scripts/validate_candidate.py"], "full")):
            with self.subTest(paths=paths):
                strategy = self.strategy(paths)
                self.assertEqual(strategy["profile"], profile)
                self.assertEqual(strategy["package_required"], profile == "full")

    def test_explicit_risks_can_only_add_to_automatic_checks(self):
        self.assertIn("package", self.strategy(["index.html"], ["package"])["risks"])
        self.assertTrue(self.strategy(["index.html"], ["lifecycle"])["package_required"])
        with self.assertRaises(ValueError):
            self.strategy(["src/rpc/sidebar.rs"], ["none"])

    def test_nul_separated_paths_preserve_embedded_whitespace(self):
        with mock.patch.object(pr_plan, "git", return_value="merge-base"), \
                mock.patch.object(pr_plan.subprocess, "check_output",
                                  return_value="src/\nfile.rs\0 index.html\0"):
            self.assertEqual(pr_plan.changed(Path("/project"), "base"),
                             ("merge-base", [" index.html", "src/\nfile.rs"]))

    def test_reports_reject_missing_skipped_stale_or_falsely_complete_results(self):
        strategy = {"profile": "business"}
        original = report(strategy)
        pr_plan.validate_report(original, strategy, SOURCE, INVENTORY)
        mutations = [
            {"results": []}, {"selected": []}, {"not_run": []},
            {"inventory": sorted(INVENTORY - {"test::git_hooks::case"})},
            {"source": {**SOURCE, "commit": "d" * 40}}, {"release_ready": True},
            {"results": [{"id": name, "status": "skip"} for name in original["selected"]]},
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                pr_plan.validate_report({**copy.deepcopy(original), **mutation},
                                        strategy, SOURCE, INVENTORY)

    def test_missing_required_ui_group_cannot_silently_reduce_coverage(self):
        with self.assertRaises(ValueError):
            pr_plan.groups(INVENTORY, "ui")


class PRDeliveryTests(unittest.TestCase):
    def setup_source(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        parent = Path(temporary.name)
        source = parent / "source"
        source.mkdir()
        return source, parent / "result"

    def test_wrong_head_is_rejected_before_running_checks_and_keeps_timing(self):
        source, destination = self.setup_source()
        with mock.patch.object(pr_delivery, "identity", return_value=SOURCE), \
                mock.patch.object(pr_delivery, "git", side_effect=["", "", "different"]), \
                mock.patch.object(pr_delivery, "execute") as execute:
            self.assertEqual(pr_delivery.validate(source, "base", destination, "requested"), 1)
        execute.assert_not_called()
        saved = json.loads((destination / "VALIDATION.json").read_text())
        self.assertFalse(saved["passed"])
        self.assertIn("finished_at", saved)
        self.assertGreaterEqual(saved["seconds"], 0)

    def test_command_start_failure_preserves_the_failed_step_and_original_error(self):
        source, destination = self.setup_source()
        with mock.patch.object(pr_delivery, "identity", return_value=SOURCE), \
                mock.patch.object(pr_delivery, "git", return_value=""), \
                mock.patch.object(pr_plan, "plan", return_value={"profile": "business"}), \
                mock.patch.object(pr_delivery, "execute", side_effect=FileNotFoundError("missing tool")):
            self.assertEqual(pr_delivery.validate(source, "base", destination), 1)
        saved = json.loads((destination / "VALIDATION.json").read_text())
        self.assertEqual(saved["steps"][0]["name"], "project-check")
        self.assertIsNone(saved["steps"][0]["returncode"])
        self.assertEqual(saved["error"]["type"], "FileNotFoundError")
        self.assertIn("missing tool", saved["error"]["detail"])

    def test_evidence_inside_source_and_reused_output_are_rejected(self):
        source, destination = self.setup_source()
        with self.assertRaises(ValueError):
            pr_delivery.validate(source, "base", source / "evidence")
        destination.mkdir()
        with self.assertRaises(FileExistsError):
            pr_delivery.validate(source, "base", destination)

    def test_records_only_selection_still_checks_the_complete_inventory(self):
        native = {"lib::application::case": ("lib::application", "case")}
        python = {"acceptance.case": {"id": "acceptance.case", "status": "pass"}}
        with mock.patch.object(rust_adapter, "executables", return_value={}), \
                mock.patch.object(rust_adapter, "inventory", return_value=native), \
                mock.patch.object(rust_adapter, "python_inventory", return_value=["acceptance.case"]), \
                mock.patch.object(rust_adapter, "run_python", return_value=python), \
                mock.patch.object(rust_adapter, "run_rust", return_value=({}, [])) as run:
            result = rust_adapter.run(
                {"protocol": 1, "tests": [*native, "acceptance.case"]},
                prefixes=[], include_python=True)
        self.assertEqual(result["selected"], ["acceptance.case"])
        self.assertEqual(result["not_run"], ["lib::application::case"])
        self.assertEqual(run.call_args.args[2], set())

    def test_foreign_project_python_tests_use_their_own_modules_in_a_fresh_process(self):
        source, _ = self.setup_source()
        for name in ("acceptance", "scripts"):
            (source / name).mkdir()
            (source / name / "__init__.py").touch()
        shutil.copy2(Path(__file__).with_name("unittest_adapter.py"),
                     source / "acceptance/unittest_adapter.py")
        (source / "scripts/validate_candidate.py").write_text("MARKER='target-project'\n")
        (source / "acceptance/test_target.py").write_text(
            "import unittest\nfrom scripts import validate_candidate\n"
            "class TargetTests(unittest.TestCase):\n"
            " def test_target(self):\n"
            "  self.assertEqual(validate_candidate.MARKER,'target-project')\n")
        with mock.patch.object(rust_adapter, "ROOT", source):
            names = rust_adapter.python_inventory()
            outcomes = rust_adapter.run_python(names)
        self.assertEqual(names, ["acceptance.test_target.TargetTests.test_target"])
        self.assertEqual(outcomes[names[0]]["status"], "pass")
