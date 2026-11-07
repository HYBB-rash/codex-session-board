import copy
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

from scripts import verify_remote_release as gate


ROOT = Path(__file__).resolve().parents[1]
COMMIT = "1" * 40
HOST = "remote:test"
LOCAL_THREAD = "11111111-1111-4111-8111-111111111111"
REMOTE_THREAD = "22222222-2222-4222-8222-222222222222"


class RemoteReleaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.base = Path(self.directory.name)
        self.root = self.base / "source"
        acceptance = self.root / "acceptance"
        acceptance.mkdir(parents=True)
        for name in ("requirements.json", "model.py", "contract.lock.json"):
            (acceptance / name).write_bytes((ROOT / "acceptance" / name).read_bytes())
        self.contract = gate.read(acceptance / "requirements.json")
        mappings = {item["id"]: ["fixture.current"] for item in self.contract["requirements"]}
        bindings = {"schema_version": 1, "fingerprint_paths": ["source.txt", "acceptance"],
                    "requirements": mappings}
        (acceptance / "bindings.json").write_text(json.dumps(bindings))
        (self.root / "source.txt").write_text("current source")
        self.binary = self.base / "final-binary"
        self.binary.write_bytes(b"synthetic executable identity; never executed")
        self.path = self.base / "manifest.json"
        self.proof = self.artifact("proof.txt", "Synthetic integrity fixture, not GUI evidence")
        self.contract_sha = gate.digest(acceptance / "requirements.json")
        self.build_sha = gate.fingerprint(self.root, bindings["fingerprint_paths"])
        self.binary_sha = gate.digest(self.binary)
        modes = [self.mode(name) for name in sorted(gate.MODES)]
        evidence = {"contract_sha256": self.contract_sha, "build_sha256": self.build_sha,
                    "mode": "legacy", "configId": self.config_id("legacy"),
                    "binary_sha256": self.binary_sha, "source_commit": COMMIT,
                    "checks": {item["id"]: self.record()
                               for item in self.contract["requirements"]
                               if item["verification"] in {"mixed", "live"}}}
        legacy = {
            "schema_version": 1, "scope": "release", "passed": True,
            "release_ready": True, "automated_passed": True,
            "contract_id": self.contract["contract_id"], "contract_sha256": self.contract_sha,
            "build_sha256": self.build_sha, "test_count": 1,
            "tests": [{"id": "fixture.current", "status": "pass"}],
            "requirements": [{"id": item["id"], "status": "pass",
                              "automated_tests": mappings[item["id"]],
                              "live_status": "pass" if item["verification"] != "automated" else None}
                             for item in self.contract["requirements"]],
        }
        self.manifest = {
            **self.record(), "schema_version": 1, "source_commit": COMMIT,
            "contract_sha256": self.contract_sha, "build_sha256": self.build_sha,
            "binary": {"path": self.binary.name, "sha256": self.binary_sha},
            "legacy_report": self.artifact("legacy.json", legacy),
            "legacy_evidence": self.artifact("legacy-evidence.json", evidence),
            "modes": modes,
            "checks": [{**self.record(), "id": identifier, "mode": mode,
                        "configId": self.config_id(mode), "binary_sha256": self.binary_sha,
                        "build_sha256": self.build_sha} for identifier, mode in gate.CASES.items()],
            "audit": {**self.record(), "outside_allowlist_forwarded_writes": 0,
                      "allowed_sessions": [{"hostId": "local", "threadId": LOCAL_THREAD},
                                           {"hostId": HOST, "threadId": REMOTE_THREAD}],
                      "forwarded_writes": [{"hostId": HOST, "threadId": REMOTE_THREAD,
                                            "method": "thread/name/set"}]},
            "cleanup": {**self.record(), "remaining_owned_resources": []},
        }
        self.review()
        self.commit_patch = patch.object(gate, "current_commit", return_value=COMMIT)
        self.commit_patch.start()
        self.addCleanup(self.commit_patch.stop)
        self.ancestry_patch = patch.object(gate, "is_ancestor",
                                           side_effect=lambda root, observed, final: observed == final)
        self.ancestry_patch.start()
        self.addCleanup(self.ancestry_patch.stop)

    def artifact(self, name, value):
        path = self.base / name
        path.write_text(json.dumps(value) if not isinstance(value, str) else value)
        return {"path": name, "sha256": gate.digest(path)}

    def record(self):
        return {"status": "pass", "operator": "fixture reviewer",
                "environment": "synthetic unit test", "notes": "Integrity checks only",
                "observed_at": datetime.now(timezone.utc).isoformat(),
                "artifacts": [self.proof]}

    def mode(self, name):
        registry = self.artifact("registry.json", {
            "codex-managed-remote-connections": [{"hostId": HOST}]})
        config = {"legacy_mode": name == "legacy", "native_sections": name != "legacy",
                  "host_id": "local", "proxy_command": [], "clock_command": [],
                  "sidebar_socket": "" if name == "legacy" else str(self.base / "guard.sock"),
                  "sidebar_state_file": str(self.base / registry["path"]),
                  "sync_sections": False, "auto_archive_seconds": None,
                  "data_dir": str(self.base / name)}
        config_id = gate.json_digest(config)[:16]
        info = {"app": "codex-session-board", "build": self.binary_sha,
                "configId": config_id, "dataDir": config["data_dir"], "pid": 123, "version": "fixture"}
        return {**self.record(), "id": name, "configId": config_id,
                "config": self.artifact(f"{name}-config.json", config),
                "info": self.artifact(f"{name}-info.json", info),
                **({"hostId": HOST, "registry": registry} if name == "remote" else {})}

    def config_id(self, name):
        return gate.json_digest(gate.read(self.base / f"{name}-config.json"))[:16]

    def review(self):
        self.manifest["review"] = {**self.record(),
                                   "subject_sha256": gate.review_digest(self.manifest)}

    def run_gate(self):
        self.path.write_text(json.dumps(self.manifest))
        return gate.verify(self.root, self.path, self.binary)

    def reject(self):
        self.review()
        with self.assertRaises((ValueError, KeyError)):
            self.run_gate()

    def change_artifact(self, reference, mutate):
        value = gate.read(self.base / reference["path"])
        mutate(value)
        (self.base / reference["path"]).write_text(json.dumps(value))
        reference["sha256"] = gate.digest(self.base / reference["path"])

    def test_complete_three_mode_evidence_passes_without_executing_binary(self):
        report, code = self.run_gate()
        self.assertEqual(code, 0)
        self.assertTrue(report["release_ready"])
        self.assertEqual({mode["configId"] for mode in report["modes"].values()},
                         {self.config_id(name) for name in gate.MODES})

    def test_context_only_final_commit_preserves_evidence_and_rejects_payload_drift(self):
        def git(*arguments, input=None):
            return subprocess.check_output(
                ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                 "-C", str(self.root), *arguments], input=input, text=True, timeout=10).strip()
        git("init", "-q")
        tree = git("mktree", input="")
        observed = git("commit-tree", tree, input="observation")
        git("update-ref", "HEAD", observed)
        self.commit_patch.stop()
        self.ancestry_patch.stop()
        self.manifest["observed_source_commit"] = self.manifest.pop("source_commit")
        self.manifest["observed_source_commit"] = observed
        self.manifest["version_preparation"] = {
            "base_commit": observed, "version": "fixture", "notes": "Prepared before observation"}
        self.change_artifact(self.manifest["legacy_evidence"],
                             lambda evidence: evidence.update(source_commit=observed))
        self.review()
        self.assertEqual(self.run_gate()[1], 0)
        original = self.path.read_bytes()
        blob = git("hash-object", "-w", "--stdin", input="archived task")
        context = git("mktree", input=f"100644 blob {blob}\ttask.md\n")
        tree = git("mktree", input=f"040000 tree {context}\tcontext\n")
        final = git("commit-tree", tree, "-p", observed, input="context archive")
        git("update-ref", "HEAD", final)
        self.assertEqual(git("diff", "--name-only", observed, final), "context/task.md")
        report, code = self.run_gate()
        self.assertEqual(code, 0)
        self.assertEqual((report["observed_source_commit"], report["final_source_commit"],
                          report["attestation_commit"]), (observed, final, final))
        self.assertEqual(self.path.read_bytes(), original)
        payload = self.base / "plugin-binary"
        payload.write_bytes(self.binary.read_bytes())
        self.assertTrue(gate.verify(self.root, self.path, payload)[0]["release_ready"])
        for changed in (self.root / "source.txt", self.binary):
            previous = changed.read_bytes()
            changed.write_bytes(b"changed payload")
            self.reject()
            changed.write_bytes(previous)
        git("update-ref", "HEAD", git("commit-tree", tree, input="unrelated history"))
        self.reject()

    def test_automated_only_or_incomplete_legacy_report_is_rejected(self):
        original = copy.deepcopy(self.manifest)
        original_report = gate.read(self.base / "legacy.json")
        for key, value in (("scope", "automated"), ("release_ready", False),
                           ("passed", False), ("automated_passed", False)):
            self.manifest = copy.deepcopy(original)
            self.manifest["legacy_report"] = self.artifact("legacy.json", original_report)
            self.change_artifact(self.manifest["legacy_report"],
                                 lambda report: report.update({key: value}))
            with self.subTest(key=key):
                self.reject()

    def test_missing_duplicate_skipped_and_failed_legacy_results_are_rejected(self):
        original = gate.read(self.base / "legacy.json")
        mutations = [
            lambda report: report["requirements"].pop(),
            lambda report: report["requirements"].append(report["requirements"][0]),
            lambda report: report["requirements"][0].update(status="pending"),
            lambda report: report["tests"].clear(),
            lambda report: report["tests"].append(report["tests"][0]),
            lambda report: report["tests"][0].update(status="skip"),
        ]
        for mutate in mutations:
            self.manifest["legacy_report"] = self.artifact("legacy.json", copy.deepcopy(original))
            self.change_artifact(self.manifest["legacy_report"], mutate)
            with self.subTest(mutation=mutate):
                self.reject()

    def test_missing_duplicate_or_skipped_extension_cases_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for action in ("missing", "duplicate", "skip", "legacy-only", "native-only"):
            self.manifest = copy.deepcopy(original)
            checks = self.manifest["checks"]
            if action == "missing":
                checks.pop()
            elif action == "duplicate":
                checks.append(checks[0])
            elif action == "skip":
                checks[0]["status"] = "skip"
            else:
                self.manifest["checks"] = [] if action == "legacy-only" else [
                    record for record in checks if record["mode"] == "native5"]
            with self.subTest(action=action):
                self.reject()

    def test_pending_and_failed_cases_never_claim_release_ready(self):
        for status, expected in (("pending", 2), ("fail", 1)):
            self.manifest["checks"][0]["status"] = status
            self.review()
            report, code = self.run_gate()
            self.assertEqual(code, expected)
            self.assertFalse(report["release_ready"])
            self.assertEqual(report["status"], status)

    def test_old_source_commit_fingerprint_and_binary_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for field in ("source_commit", "observed_source_commit", "build_sha256", "contract_sha256"):
            self.manifest = copy.deepcopy(original)
            self.manifest[field] = "0" * 64
            with self.subTest(field=field):
                self.reject()
        self.manifest = original
        (self.root / "source.txt").write_text("changed source")
        self.reject()

    def test_changed_or_substituted_final_binary_is_rejected(self):
        self.binary.write_bytes(b"replacement")
        self.reject()
        self.binary.write_bytes(b"synthetic executable identity; never executed")
        other = self.base / "old-binary"
        other.write_bytes(b"old executable")
        self.manifest["binary"]["path"] = other.name
        self.reject()

    def test_case_mode_config_binary_and_source_must_match(self):
        original = copy.deepcopy(self.manifest)
        for key, value in (("mode", "legacy"), ("configId", "0" * 16),
                           ("binary_sha256", "0" * 64), ("build_sha256", "0" * 64)):
            self.manifest = copy.deepcopy(original)
            self.manifest["checks"][0][key] = value
            with self.subTest(field=key):
                self.reject()
        original_evidence = gate.read(self.base / "legacy-evidence.json")
        for key, value in (("mode", "remote"), ("configId", "old"),
                           ("binary_sha256", "old"), ("source_commit", "old")):
            self.manifest = copy.deepcopy(original)
            self.manifest["legacy_evidence"] = self.artifact(
                "legacy-evidence.json", original_evidence)
            self.change_artifact(self.manifest["legacy_evidence"],
                                 lambda evidence: evidence.update({key: value}))
            with self.subTest(legacy_field=key):
                self.reject()

    def test_wrong_mode_flags_config_identity_or_runtime_build_is_rejected(self):
        native = next(mode for mode in self.manifest["modes"] if mode["id"] == "native5")
        self.change_artifact(native["config"], lambda config: config.update(legacy_mode=True))
        self.reject()

    def test_missing_modes_wrong_runtime_identity_and_disabled_target_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        original_info = gate.read(self.base / original["modes"][0]["info"]["path"])
        for field, value in (("configId", "old"), ("build", "0" * 64),
                             ("dataDir", "/wrong"), ("pid", True)):
            self.manifest = copy.deepcopy(original)
            mode = self.manifest["modes"][0]
            mode["info"] = self.artifact(mode["info"]["path"], original_info)
            self.change_artifact(mode["info"], lambda info: info.update({field: value}))
            with self.subTest(field=field):
                self.reject()
        self.manifest = copy.deepcopy(original)
        self.manifest["modes"].pop()
        self.reject()
        self.manifest = original
        self.artifact(original["modes"][0]["info"]["path"], original_info)
        mode = next(mode for mode in self.manifest["modes"] if mode["id"] == "remote")
        self.change_artifact(mode["registry"], lambda registry: registry.update(
            {"remote-connection-auto-connect-by-host-id": {HOST: False}}))
        self.reject()

    def test_damaged_or_missing_attachments_and_legacy_evidence_stay_pending(self):
        reference = self.manifest["legacy_evidence"]
        original = (self.base / reference["path"]).read_bytes()
        self.change_artifact(reference, lambda evidence: evidence["checks"]["STATE-001"].update(
            operator=""))
        self.review()
        self.assertEqual(self.run_gate()[1], 2)
        (self.base / reference["path"]).write_bytes(original)
        reference["sha256"] = gate.digest(self.base / reference["path"])
        self.review()
        (self.base / "proof.txt").write_text("damaged")
        report, code = self.run_gate()
        self.assertEqual(code, 2)
        self.assertFalse(report["release_ready"])
        (self.base / "proof.txt").unlink()
        self.assertEqual(self.run_gate()[1], 2)

    def test_allowlist_escape_false_zero_and_unfinished_cleanup_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for action in ("count", "boolean", "write", "duplicate", "cleanup"):
            self.manifest = copy.deepcopy(original)
            audit = self.manifest["audit"]
            if action in {"count", "boolean"}:
                audit["outside_allowlist_forwarded_writes"] = 1 if action == "count" else False
            elif action == "write":
                audit["forwarded_writes"][0]["hostId"] = "remote:other"
            elif action == "duplicate":
                audit["allowed_sessions"].append(audit["allowed_sessions"][0])
            else:
                self.manifest["cleanup"]["remaining_owned_resources"] = ["proxy-pid"]
            with self.subTest(action=action):
                self.reject()

    def test_stale_review_future_time_and_missing_operator_cannot_pass(self):
        self.manifest["checks"][0]["notes"] = "unreviewed change"
        with self.assertRaises(ValueError):
            self.run_gate()
        self.manifest["checks"][0]["observed_at"] = "2999-01-01T00:00:00+00:00"
        self.reject()
        self.manifest["checks"][0]["observed_at"] = datetime.now(timezone.utc).isoformat()
        self.manifest["cleanup"]["observed_at"] = datetime.now(timezone.utc).isoformat()
        self.manifest["checks"][0]["operator"] = ""
        self.review()
        self.assertEqual(self.run_gate()[1], 2)

    def test_duplicate_json_keys_and_changed_frozen_contract_are_rejected(self):
        self.path.write_text('{"schema_version":1,"schema_version":1}')
        with self.assertRaises(ValueError):
            gate.read(self.path)
        (self.root / "acceptance/model.py").write_text("changed frozen model")
        self.reject()

    def test_report_cannot_overwrite_inputs_attachments_or_fingerprinted_sources(self):
        legacy_only = self.artifact("legacy-only.txt", "legacy attachment")
        self.change_artifact(self.manifest["legacy_evidence"], lambda evidence:
                             evidence["checks"]["STATE-001"].update(artifacts=[legacy_only]))
        self.path.write_text(json.dumps(self.manifest))
        alias = self.base / "binary-alias"
        alias.hardlink_to(self.binary)
        targets = [self.path, self.binary, alias, self.base / "native5-config.json",
                   self.base / "remote-info.json", self.base / "proof.txt",
                   self.base / "legacy.json", self.base / "legacy-only.txt",
                   self.base / "legacy-evidence.json", self.base / "registry.json",
                   self.root / "source.txt", self.root / "acceptance/new-report.json"]
        for target in targets:
            before = target.read_bytes() if target.exists() else None
            with self.subTest(target=target), self.assertRaises(ValueError):
                gate.protect_report(self.root, self.path, self.manifest, target)
            self.assertEqual(target.read_bytes() if target.exists() else None, before)

    def test_cli_rejects_unsafe_report_without_changing_input(self):
        self.path.write_text(json.dumps(self.manifest))
        before = self.path.read_bytes()
        process = subprocess.run(
            [sys.executable, str(ROOT / "scripts/verify_remote_release.py"),
             "--root", str(self.root), "--manifest", str(self.path),
             "--binary", str(self.binary), "--report", str(self.path)],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(process.returncode, 1)
        self.assertFalse(json.loads(process.stdout)["release_ready"])
        self.assertEqual(self.path.read_bytes(), before)
