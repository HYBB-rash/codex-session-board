import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from scripts import integration_plan as planner
from scripts import verify_integration


class IntegrationPlanTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.patch = mock.patch.object(planner, "ROOT", self.root)
        self.patch.start()
        self.addCleanup(self.patch.stop)
        self.git("init", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.write("index.html", "before")
        self.write("Cargo.toml", '[package]\nname="codex-session-board"\nversion="0.3.79"\n')
        self.write("Cargo.lock", '[[package]]\nname="codex-session-board"\nversion="0.3.79"\n')
        self.base = self.commit()

    def git(self, *arguments):
        return subprocess.check_output(["git", *arguments], cwd=self.root,
                                       stderr=subprocess.PIPE, text=True).strip()

    def write(self, path, contents):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(contents)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def test_ui_scope_rejects_unknown_dependency_deletion_and_mode_changes(self):
        for change in ("unknown", "dependency", "deletion", "symlink", "executable"):
            with self.subTest(change=change):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fd")
                self.write("index.html", "after")
                if change == "unknown":
                    self.write("unexpected.json", "{}")
                elif change == "dependency":
                    self.write("Cargo.toml", '[package]\nversion="0.3.80"\n[dependencies]\nnew="1"\n')
                elif change == "deletion":
                    (self.root / "index.html").unlink()
                elif change == "symlink":
                    (self.root / "index.html").unlink()
                    (self.root / "index.html").symlink_to("Cargo.toml")
                else:
                    (self.root / "index.html").chmod(0o755)
                self.commit()
                self.assertEqual(planner.plan(self.base)["profile"], "full")

    def test_only_version_fields_are_exempted(self):
        self.write("index.html", "after")
        self.write("Cargo.toml", '[package]\nname="codex-session-board"\nversion="0.3.80"\n')
        self.write("Cargo.lock", '[[package]]\nname="codex-session-board"\nversion="0.3.80"\n')
        self.commit()
        self.assertEqual(planner.plan(self.base)["profile"], "ui")
        self.write("Cargo.lock", '[[package]]\nname="codex-session-board"\nversion="0.3.80"\nchecksum="changed"\n')
        self.commit()
        self.assertEqual(planner.plan(self.base)["profile"], "full")

    def test_replay_cannot_invent_an_ingress_or_hide_actual_changes(self):
        self.write("context/integration-sources.json", json.dumps({
            "base": self.base, "ingress": self.base, "replay_base": self.base,
            "source_head": self.base,
        }))
        self.commit()
        with self.assertRaises(ValueError):
            planner.plan(self.base)

    def test_special_unknown_paths_never_become_ui_paths(self):
        for name in (" index.html", "\nindex.html", "index.html\n"):
            with self.subTest(name=name):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fd")
                self.write("index.html", "after")
                self.write(name, "unknown")
                self.commit()
                self.assertIn(name, planner.changed(self.base, "HEAD"))
                self.assertEqual(planner.plan(self.base)["profile"], "full")

    def test_wrong_working_directory_cannot_verify_another_implementation(self):
        with mock.patch.object(verify_integration.rust_adapter, "ROOT", self.root):
            with self.assertRaises(ValueError):
                verify_integration.verify(self.base)

    def test_real_replay_retains_ui_scope_even_when_main_already_has_the_pr(self):
        original_branch = self.git("branch", "--show-current")
        self.git("switch", "-c", "source")
        self.write("index.html", "after")
        source = self.commit()
        self.git("switch", original_branch)
        self.git("merge", "--no-ff", "-m", "original merge", source)
        main = self.git("rev-parse", "HEAD")
        self.git("switch", "-c", "replay", self.base)
        self.git("merge", "--no-ff", "-m", "replay merge", source)
        ingress = self.git("rev-parse", "HEAD")
        self.git("merge", "--no-ff", "-m", "join main", main)
        self.write("context/integration-sources.json", json.dumps({
            "base": main, "ingress": ingress, "replay_base": self.base, "source_head": source,
        }))
        self.commit()
        strategy = planner.plan(main)
        self.assertEqual(strategy["profile"], "ui")
        self.assertIn("index.html", strategy["paths"])
        self.write("unknown.json", "{}")
        self.commit()
        self.assertEqual(planner.plan(main)["profile"], "full")

    def test_release_record_must_bind_the_exact_parent_and_preserve_other_rules(self):
        self.write("project-rules.json", '{"versioning":{"feature_releases":[]},"rule":true}')
        base = self.commit()
        self.write("index.html", "after")
        parent = self.commit()
        self.write("Cargo.toml", '[package]\nname="codex-session-board"\nversion="0.3.80"\n')
        record = {"versioning": {"feature_releases": [{
            "after_commit": parent, "from": "0.3.79", "to": "0.3.80", "decision": "冻结发布候选",
        }]}, "rule": True}
        self.write("project-rules.json", json.dumps(record))
        self.commit()
        self.assertEqual(planner.plan(base)["profile"], "ui")
        record["rule"] = False
        self.write("project-rules.json", json.dumps(record))
        self.commit()
        self.assertEqual(planner.plan(base)["profile"], "full")

    def test_partial_results_cannot_be_presented_as_complete_or_successful(self):
        inventory = {prefix + "::case" for prefix in planner.UI_GROUPS}
        inventory.add("example::project_check::case")
        plan = {"profile": "ui"}
        chosen = planner.selected(inventory, "ui")
        report = {
            "scope": "integration", "plan": plan, "source": {}, "passed": True,
            "release_ready": False, "inventory": sorted(inventory), "selected": sorted(chosen),
            "not_run": sorted(inventory - chosen),
            "tests": [{"id": name, "status": "pass"} for name in sorted(chosen)],
        }
        planner.validate_report(report, plan, {}, inventory)
        mutations = (("release_ready", True), ("not_run", []), ("selected", []),
                     ("tests", []), ("plan", {"profile": "full"}))
        for key, value in mutations:
            with self.subTest(key=key), self.assertRaises(ValueError):
                planner.validate_report({**report, key: value}, plan, {}, inventory)
        for status in ("skip", "fail", "error"):
            changed = copy.deepcopy(report)
            changed["tests"][0]["status"] = status
            with self.subTest(status=status), self.assertRaises(ValueError):
                planner.validate_report(changed, plan, {}, inventory)
        inventory.add("acceptance.application.case")
        plan = {"profile": "application"}
        chosen = planner.selected(inventory, "application")
        report.update(plan=plan, inventory=sorted(inventory), selected=sorted(chosen),
                      not_run=sorted(inventory - chosen),
                      tests=[{"id": name, "status": "pass"} for name in sorted(chosen)])
        planner.validate_report(report, plan, {}, inventory)
        report["tests"] = [test for test in report["tests"] if "::" in test["id"]]
        with self.assertRaises(ValueError):
            planner.validate_report(report, plan, {}, inventory)

    def test_new_application_files_keep_python_and_exclude_unrelated_hook_self_tests(self):
        self.write("src/added.rs", "pub fn added() {}")
        self.write("scripts/native_new/added.py", "value = 1")
        self.write("acceptance/test_added.py", "value = 1")
        self.write("index.html", "mixed application change")
        self.write("Docs/NEW-FEATURE.md", "new feature")
        application = self.commit()
        self.assertEqual(planner.plan(self.base)["profile"], "application")
        inventory = {"lib::application::case", "test::git_hooks::case",
                     "example::project_check::case", "acceptance.test_added.case"}
        self.assertEqual(planner.selected(inventory, "application"),
                         {"lib::application::case", "acceptance.test_added.case"})
        bindings = json.loads((verify_integration.ROOT / "acceptance/bindings.json").read_text())
        actual = planner.bound_inventory(bindings, sys.platform)
        chosen = planner.selected(actual, "application")
        outcome = {"results": [{"id": name, "status": "pass"} for name in sorted(chosen)],
                   "inventory": sorted(actual), "selected": sorted(chosen),
                   "not_run": sorted(actual - chosen), "timings": []}
        with mock.patch.object(planner, "plan", return_value={"profile": "application"}), \
                mock.patch.object(verify_integration.rust_adapter, "run", return_value=outcome) as run:
            report = verify_integration.verify(self.base)
        self.assertTrue(report["passed"])
        self.assertTrue(run.call_args.kwargs["include_python"])
        self.assertTrue(all("::" in prefix for prefix in run.call_args.kwargs["prefixes"]))
        for path in ("scripts/check", "scripts/new-unknown.mjs", "tests/git_hooks.rs",
                     "acceptance/unittest_adapter.py", "examples/dev_fixture_unrelated.rs"):
            with self.subTest(path=path):
                self.git("reset", "--hard", application)
                self.git("clean", "-fd")
                self.write(path, "changed")
                self.commit()
                self.assertEqual(planner.plan(self.base)["profile"], "full")

    def test_application_scope_rejects_deletion_and_mode_changes(self):
        self.write("src/existing.rs", "pub fn existing() {}")
        base = self.commit()
        for change in ("deletion", "symlink", "executable"):
            with self.subTest(change=change):
                self.git("reset", "--hard", base)
                path = self.root / "src/existing.rs"
                if change == "deletion":
                    path.unlink()
                elif change == "symlink":
                    path.unlink()
                    path.symlink_to("../Cargo.toml")
                else:
                    path.chmod(0o755)
                self.commit()
                self.assertEqual(planner.plan(base)["profile"], "full")
