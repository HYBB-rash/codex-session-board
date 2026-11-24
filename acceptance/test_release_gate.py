import copy
from datetime import datetime, timedelta, timezone
import io
import json
from pathlib import Path
import tarfile
from tempfile import TemporaryDirectory
import unittest

from delivery_checks import CONTENTS, EXCLUSIONS, INTEGRITY, REQUIRED_FILES, delivery_checks
from release_gate import INSTALLATION_ASSERTIONS, PLATFORMS, archive_arguments, delivery_results, evaluate, tap_results
from release_identity import (
    BASELINE, CONTRACT, FROZEN, PLUGIN_MANIFEST, archive_identity, plugin_identity, sha256,
    stream_sha256,
)


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        temporary = TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        artifact = self.root / "native.log"
        artifact.write_text("pointer drop, original sidebar read back matching partition\n")
        self.now = datetime(2026, 11, 22, 12, tzinfo=timezone.utc)
        self.requirements = [
            {"id": "UNIT-001", "title": "刷新", "verification": "automated", "then": ["版本保持"]},
            {"id": "LIVE-001", "title": "拖动", "verification": "mixed", "then": ["原位读回", "备注保持"]},
        ]
        self.bindings = {"UNIT-001": [["refresh"]], "LIVE-001": [["move"], ["move"]]}
        self.tests = {"refresh": "pass", "move": "pass"}
        self.identity = {
            "contract": {"id": CONTRACT, "sha256": FROZEN["requirements.json"], "baseline": BASELINE},
            "candidate": {"sourceSha256": "1" * 64, "testRuntimeSha256": "2" * 64},
            "archives": {
                platform: {"version": "0.3.8", "archiveSha256": str(index) * 64, "runtimeSha256": "7" * 64}
                for index, platform in enumerate(PLATFORMS, start=3)
            },
            "archiveErrors": {},
        }
        self.evidence = {
            "schemaVersion": 1,
            "contract": copy.deepcopy(self.identity["contract"]),
            "sourceSha256": self.identity["candidate"]["sourceSha256"],
            "runs": [
                {
                    "requirement": "LIVE-001", "platform": platform,
                    **self.identity["archives"][platform],
                    "operator": "桌面验收员", "observedAt": self.now.isoformat(),
                    "host": {"os": scope[0], "arch": scope[1], "machine": "测试机", "codexBuild": "26.1111.71704"},
                    "assertions": [
                        {"index": index, "status": "pass", "observation": "原生读回一致",
                         "artifacts": [{"path": artifact.name, "sha256": sha256(artifact)}]}
                        for index in (1, 2)
                    ],
                }
                for platform, scope in PLATFORMS.items()
            ],
        }
        self.evidence["installationRuns"] = [
            {
                **{key: value for key, value in next(
                    run for run in self.evidence["runs"] if run["platform"] == platform
                ).items() if key not in {"requirement", "assertions"}},
                "mode": mode,
                "assertions": [
                    {"index": index, "status": "pass", "observation": "安装方式实测结果",
                     "artifacts": [{"path": artifact.name, "sha256": sha256(artifact)}]}
                    for index in range(1, len(clauses) + 1)
                ],
            }
            for (mode, platform), clauses in INSTALLATION_ASSERTIONS.items()
        ]

    def evaluate(self, evidence):
        return evaluate(
            self.requirements, self.bindings, self.tests, self.identity, evidence, self.root, self.now,
        )

    def live_result(self, result, platform="linux-x86_64", index=0):
        return result["requirements"][1]["assertions"][index]["live"][platform]

    def test_complete_evidence_releases_a_candidate(self):
        result = self.evaluate(self.evidence)
        self.assertTrue(result["releaseReady"], result)
        self.assertEqual([row["status"] for row in result["requirements"]], ["pass", "pass"])
        self.assertEqual([row["status"] for row in result["installations"]], ["pass"] * 4)

    def test_frozen_requirements_cannot_release_without_macos_in_place_evidence(self):
        runs = self.evidence["installationRuns"]
        mac_in_place = next(run for run in runs if (run["mode"], run["platform"]) == ("in-place", "macos-arm64"))
        runs.remove(mac_in_place)
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertTrue(all(row["status"] == "pass" for row in result["requirements"]))
        self.assertEqual(result["installations"][-1]["status"], "pending")
        del self.evidence["installationRuns"]
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertTrue(all(row["status"] == "pending" for row in result["installations"]))

    def test_installation_evidence_checks_scope_and_attachment(self):
        run = self.evidence["installationRuns"][-1]
        run["assertions"][0]["artifacts"][0]["sha256"] = "0" * 64
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("SHA256", result["installations"][-1]["assertions"][0]["detail"])
        run["assertions"][0]["status"] = "fail"
        self.assertEqual(self.evaluate(self.evidence)["installations"][-1]["status"], "fail")
        run["mode"] = "copy"
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("重复", result["errors"][0])

    def test_automated_success_without_live_evidence_is_pending(self):
        result = self.evaluate(None)
        self.assertFalse(result["releaseReady"])
        self.assertEqual(result["requirements"][0]["status"], "pass")
        self.assertIn("缺少", self.live_result(result)["detail"])

    def test_missing_binding_and_missing_result_are_reported_separately(self):
        self.bindings["LIVE-001"][0] = []
        del self.tests["refresh"]
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertEqual(result["requirements"][0]["assertions"][0]["detail"], "缺少自动断言结果")
        self.assertEqual(result["requirements"][1]["assertions"][0]["detail"], "缺少自动断言绑定")

    def test_any_failed_or_skipped_automated_assertion_blocks_release(self):
        for status in ("fail", "error", "skip"):
            with self.subTest(status=status):
                self.tests["refresh"] = status
                result = self.evaluate(self.evidence)
                self.assertFalse(result["releaseReady"])
                self.assertEqual(result["requirements"][0]["status"], "fail")
                self.assertEqual(result["requirements"][0]["assertions"][0]["automated"][0]["status"], status)

    def test_expired_future_and_timezone_free_observations_are_pending(self):
        for observed_at in (
            self.now - timedelta(days=31), self.now + timedelta(seconds=1),
            self.now.replace(tzinfo=None),
        ):
            with self.subTest(observed_at=observed_at):
                self.evidence["runs"][0]["observedAt"] = observed_at.isoformat()
                result = self.evaluate(self.evidence)
                self.assertFalse(result["releaseReady"])
                self.assertIn("过期", self.live_result(result)["detail"])

    def test_source_and_contract_drift_invalidate_existing_evidence(self):
        self.evidence["sourceSha256"] = "a" * 64
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("改变", result["errors"][0])
        self.evidence["sourceSha256"] = self.identity["candidate"]["sourceSha256"]
        self.evidence["contract"]["id"] = "another-contract/version"
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("契约", result["errors"][0])

    def test_platform_build_runtime_and_archive_scopes_cannot_be_swapped(self):
        original = copy.deepcopy(self.evidence)
        for field, value in (("version", "0.3.0"), ("archiveSha256", "f" * 64), ("runtimeSha256", "f" * 64)):
            with self.subTest(field=field):
                self.evidence = copy.deepcopy(original)
                self.evidence["runs"][0][field] = value
                result = self.evaluate(self.evidence)
                self.assertFalse(result["releaseReady"])
                self.assertIn("指纹不符", self.live_result(result)["detail"])
        self.evidence = original
        self.evidence["runs"][0]["host"]["arch"] = "arm64"
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("宿主平台", self.live_result(result)["detail"])

    def test_missing_or_failed_live_assertions_remain_visible(self):
        self.evidence["runs"][0]["assertions"].pop()
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertEqual(self.live_result(result, index=1)["status"], "pending")
        self.evidence["runs"][0]["assertions"][0]["status"] = "fail"
        result = self.evaluate(self.evidence)
        self.assertEqual(self.live_result(result)["status"], "fail")

    def test_attachment_tamper_absence_traversal_and_symlinks_are_rejected(self):
        artifact = self.root / "native.log"
        artifact.write_text("changed")
        self.assertIn("SHA256", self.live_result(self.evaluate(self.evidence))["detail"])
        artifact.unlink()
        self.assertIn("不存在", self.live_result(self.evaluate(self.evidence))["detail"])
        artifact.symlink_to(__file__)
        self.assertIn("符号链接", self.live_result(self.evaluate(self.evidence))["detail"])
        self.evidence["runs"][0]["assertions"][0]["artifacts"][0]["path"] = "../outside.log"
        self.assertIn("越界", self.live_result(self.evaluate(self.evidence))["detail"])

    def test_duplicate_scope_and_wrong_requirement_are_invalid(self):
        self.evidence["runs"].append(copy.deepcopy(self.evidence["runs"][0]))
        result = self.evaluate(self.evidence)
        self.assertFalse(result["releaseReady"])
        self.assertIn("重复", result["errors"][0])
        self.evidence["runs"][-1]["requirement"] = "UNIT-001"
        self.assertIn("范围错误", self.evaluate(self.evidence)["errors"][0])

    def test_skip_and_todo_in_tap_never_appear_as_pass(self):
        tap = "\n".join(("ok 1 - passing", "not ok 2 - failing", "ok 3 - skipped # SKIP pending",
                         "ok 4 - todo # TODO unimplemented", "    ok 1 - nested", "1..4"))
        self.assertEqual(
            list(tap_results(tap)),
            [("passing", "pass"), ("failing", "fail"), ("skipped", "skip"), ("todo", "skip")],
        )

    def test_archive_platform_options_cannot_repeat(self):
        with self.assertRaisesRegex(ValueError, "唯一"):
            archive_arguments(["macos-arm64=first", "macos-arm64=second"])


class ArchiveVerificationTests(unittest.TestCase):
    def setUp(self):
        temporary = TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name, "package.tar.gz")
        self.source = {"src/board.ts": b"source"}
        self.runtime = {"runtime/main.js": b"candidate", "board-assets/version.txt": b"0.3.8\n"}

    def digests(self, entries):
        return {
            name: plugin_identity(data)[0] if name == PLUGIN_MANIFEST else stream_sha256(io.BytesIO(data))
            for name, data in entries.items()
        }

    def archive(self, files=None, extra=None):
        manifest = {
            "schema": 1, "version": "0.3.8", "platform": "linux-x86_64",
            "files": [{"path": name, "sha256": digest} for name, digest in self.digests(self.runtime).items()],
        }
        contents = {
            "candidate/release.json": json.dumps(manifest).encode(),
            **{f"candidate/{name}": data for name, data in self.source.items()},
            **{f"candidate/dist/{name}": data for name, data in (files or self.runtime).items()},
            **(extra or {}),
        }
        with tarfile.open(self.path, "w:gz") as package:
            for name, data in contents.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                package.addfile(info, io.BytesIO(data))

    def identify(self, platform="linux-x86_64", source=None, runtime=None):
        return archive_identity(self.path, platform, self.digests(source or self.source), self.digests(runtime or self.runtime))

    def test_archive_is_bound_to_bytes_and_current_runtime(self):
        self.archive()
        identified = self.identify()
        self.assertEqual(identified["archiveSha256"], sha256(self.path))
        with self.assertRaisesRegex(ValueError, "自动测试产物不同"):
            self.identify(runtime={**self.runtime, "runtime/main.js": b"other"})
        with self.assertRaisesRegex(ValueError, "归档源码"):
            self.identify(source={"src/board.ts": b"new source"})
        with self.assertRaisesRegex(ValueError, "平台范围"):
            self.identify("macos-arm64")

    def test_modified_archived_payload_and_traversal_fails(self):
        self.archive(files={**self.runtime, "runtime/main.js": b"tampered"})
        with self.assertRaisesRegex(ValueError, "SHA256 清单"):
            self.identify()
        self.archive(extra={"candidate/../outside": b"invalid"})
        with self.assertRaisesRegex(ValueError, "越界"):
            self.identify()

    def test_workspace_only_live_tool_fails_delivery_exclusions(self):
        tool = "acceptance/live/board-accept.py"
        self.archive(extra={f"candidate/{tool}": b"live tool"})
        self.assertIn(tool, self.identify()["deliveryChecks"][EXCLUSIONS])

    def test_packaged_plugin_version_and_mcp_entry_are_bound(self):
        self.source[PLUGIN_MANIFEST] = b'{"version":"0.3.7","name":"fixture"}'
        self.source[".mcp.json"] = b'{"mcpServers":{}}'
        plugin = f"candidate/{PLUGIN_MANIFEST}"
        self.archive(extra={plugin: b'{"version":"0.3.8","name":"fixture"}'})
        self.assertEqual(self.identify()["version"], "0.3.8")
        self.archive(extra={plugin: b'{"version":"0.3.7","name":"fixture"}'})
        with self.assertRaisesRegex(ValueError, "插件版本"):
            self.identify()
        self.archive(extra={plugin: b'{"version":"0.3.8","name":"other"}'})
        with self.assertRaisesRegex(ValueError, "归档源码"):
            self.identify()
        self.archive(extra={plugin: b'{"version":"0.3.8","name":"fixture"}',
                            "candidate/.mcp.json": b'{"mcpServers":{"other":{}}}'})
        with self.assertRaisesRegex(ValueError, "归档源码"):
            self.identify()


class DeliveryChecksTests(unittest.TestCase):
    def setUp(self):
        temporary = TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.archive = Path(temporary.name, "candidate.tar.gz")
        self.inventory = dict.fromkeys(REQUIRED_FILES, "same-content")
        self.checksum = "a" * 64
        self.receipt = self.archive.with_suffix(".gz.sha256")
        self.receipt.write_text(f"{self.checksum}  {self.archive.name}\n")

    def test_complete_inventory_includes_migration_and_public_checksum(self):
        result = delivery_checks(self.archive, self.checksum, self.inventory, self.inventory.keys(), True)
        self.assertEqual(result, {CONTENTS: "", EXCLUSIONS: "", INTEGRITY: ""})
        del self.inventory["dist/board-assets/mcp.html"]
        result = delivery_checks(self.archive, self.checksum, self.inventory, self.inventory.keys(), False)
        self.assertIn("mcp.html", result[CONTENTS])
        self.assertIn("迁移和回退", result[CONTENTS])

    def test_injected_data_paths_and_missing_or_corrupt_checksum_are_rejected(self):
        added = {
            "node_modules/dependency", "dist/board-assets/config.json", "dist/db/status.sqlite3-wal",
            "state/session-board/records.json", "ui/previous.png", "docs/fixture.jsonl",
            "dist/cache", "acceptance/__pycache__", "acceptance/.untracked/snapshot", "unexpected.txt",
        }
        self.receipt.write_text(f"{'b' * 64}  {self.archive.name}\n")
        result = delivery_checks(self.archive, self.checksum, self.inventory, self.inventory.keys() | added, True)
        for name in added:
            with self.subTest(name=name):
                self.assertIn(name, result[EXCLUSIONS])
        self.assertIn("匹配", result[INTEGRITY])
        self.receipt.unlink()
        result = delivery_checks(self.archive, self.checksum, self.inventory, self.inventory.keys(), True)
        self.assertIn("匹配", result[INTEGRITY])

    def test_delivery_needs_both_candidate_archives_and_keeps_failed_clause(self):
        valid = {CONTENTS: "", EXCLUSIONS: "", INTEGRITY: ""}
        identity = {"archives": {"linux-x86_64": {"deliveryChecks": valid}}, "archiveErrors": {}}
        self.assertEqual(delivery_results(identity), {})
        identity["archives"]["macos-arm64"] = {"deliveryChecks": {**valid, EXCLUSIONS: "leaked data"}}
        self.assertEqual(delivery_results(identity), {
            CONTENTS: "pass", EXCLUSIONS: "fail", INTEGRITY: "pass",
        })
        identity["archives"]["macos-arm64"]["deliveryChecks"] = valid
        self.assertEqual(delivery_results(identity), dict.fromkeys(valid, "pass"))


if __name__ == "__main__":
    unittest.main()
