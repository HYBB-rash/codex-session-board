import json
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import tarfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import publish_plugin


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.package = self.root / "package"
        (self.package / "bin").mkdir(parents=True)
        (self.package / ".codex-plugin").mkdir()
        (self.package / "bin/codex-session-board").write_bytes(b"candidate executable")
        (self.package / "bin/run-plugin").write_text("launcher")
        for name in ("codex-session-board", "run-plugin"):
            (self.package / "bin" / name).chmod(0o755)
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            (self.package / name).write_text(json.dumps({"version": "0.2.93"}))
        self.manifest = {
            "schema_version": 1, "target": "x86_64-unknown-linux-gnu", "version": "0.2.93",
            "source_commit": "a" * 40, "build_sha256": "b" * 64,
            "files": package_plugin.hashes(self.package),
        }
        self.manifest["binary_sha256"] = self.manifest["files"]["bin/codex-session-board"]
        self.manifest["contract_sha256"] = hashlib.sha256(b"contract").hexdigest()
        self.write_manifest()
        (self.root / "acceptance").mkdir()
        (self.root / "acceptance/bindings.json").write_text('{"fingerprint_paths":["source"]}')
        (self.root / "acceptance/requirements.json").write_bytes(b"contract")
        (self.root / "Cargo.toml").write_text('[package]\nname="codex-session-board"\nversion="0.2.93"\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname="codex-session-board"\nversion="0.2.93"\n')
        (self.root / ".codex-plugin").mkdir()
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            (self.root / name).write_text((self.package / name).read_text())

    def write_manifest(self):
        (self.package / "RELEASE.json").write_text(json.dumps(self.manifest))

    def platform_package(self, target, version):
        package = self.root / f"{target}-{version}"
        shutil.copytree(self.package, package)
        for name in ("plugin.json", ".codex-plugin/plugin.json"):
            (package / name).write_text(json.dumps({"version": version}))
        manifest = {**self.manifest, "target": target, "version": version,
                    "files": package_plugin.hashes(package)}
        (package / "RELEASE.json").write_text(json.dumps(manifest))
        return package

    def test_platform_channel_updates_are_atomic_and_retries_reuse_commits(self):
        repository = self.root / "remote.git"
        subprocess.run(["git", "init", "--bare", "-q", str(repository)], check=True)
        targets = ("aarch64-apple-darwin", self.manifest["target"])
        def references():
            return package_plugin.command(["git", "ls-remote", "--heads", str(repository)])
        environment = {
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_COUNT": "3",
            "GIT_CONFIG_KEY_0": "user.name", "GIT_CONFIG_VALUE_0": "Release fixture",
            "GIT_CONFIG_KEY_1": "user.email", "GIT_CONFIG_VALUE_1": "fixture@example.invalid",
            "GIT_CONFIG_KEY_2": "commit.gpgsign", "GIT_CONFIG_VALUE_2": "false",
        }
        with patch.dict(os.environ, environment), \
                patch.object(publish_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                patch.object(publish_plugin.shutil, "which", return_value=None):
            packages = [(self.platform_package(target, "0.2.93"), None) for target in targets]
            publish_plugin.publish_many(str(repository), packages, "candidate")
            previous = references()
            packages = [(self.platform_package(target, "0.2.94"), None) for target in targets]
            contents = {str(package): (package / "RELEASE.json").read_bytes() for package, _ in packages}
            with patch.object(publish_plugin, "remote_revisions", return_value={}):
                with self.assertRaisesRegex(ValueError, "changed while preparing"):
                    publish_plugin.publish_many(str(repository), packages, "candidate")
            self.assertEqual(references(), previous)
            hook = repository / "hooks/update"
            hook.write_text(f'#!/bin/sh\n[ "$1" != "refs/heads/candidate/{targets[1]}" ]\n')
            hook.chmod(0o755)
            with self.assertRaises(subprocess.CalledProcessError):
                publish_plugin.publish_many(str(repository), packages, "candidate")
            self.assertEqual(references(), previous)
            hook.unlink()
            original_run = subprocess.run
            with patch.object(publish_plugin.subprocess, "run", wraps=original_run) as run:
                published = publish_plugin.publish_many(str(repository), packages, "candidate")
            pushes = [call.args[0] for call in run.call_args_list if call.args[0][:2] == ["git", "push"]]
            self.assertEqual(len(pushes), 1)
            self.assertIn("--atomic", pushes[0])
            updated = references()
            remote = dict(line.split()[::-1] for line in updated.splitlines())
            self.assertEqual(set(remote), {f"refs/heads/candidate/{target}" for target in targets})
            self.assertEqual(remote, {f"refs/heads/{entry['reference']}": entry["revision"] for entry in published})
            self.assertTrue(all(entry["revision"] not in previous for entry in published))
            self.assertEqual(publish_plugin.publish_many(str(repository), packages, "candidate"), published)
            self.assertEqual(references(), updated)
            self.assertEqual({str(package): (package / "RELEASE.json").read_bytes() for package, _ in packages}, contents)
            single = publish_plugin.publish(str(repository), packages[0][0], "candidate")
            self.assertEqual(single, published[0])

    def test_second_platform_rejection_cannot_change_either_channel(self):
        second = self.platform_package("aarch64-apple-darwin", "0.2.93")
        with patch.object(publish_plugin, "authorize", side_effect=[self.manifest, ValueError("second platform")]), \
                patch.object(publish_plugin, "prepare") as prepare:
            with self.assertRaisesRegex(ValueError, "second platform"):
                publish_plugin.publish_many("unused", [(self.package, None), (second, None)], "candidate")
            prepare.assert_not_called()
        for packages in (
            [(self.package, None), (self.package, None)],
            [(self.package, None), (self.platform_package("aarch64-apple-darwin", "0.2.94"), None)],
        ):
            with patch.object(publish_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                    patch.object(publish_plugin, "prepare") as prepare:
                with self.assertRaises(ValueError):
                    publish_plugin.publish_many("unused", packages, "candidate")
                prepare.assert_not_called()

    def test_bulk_cli_preserves_legacy_single_package_and_requires_both_records(self):
        second = self.platform_package("aarch64-apple-darwin", "0.2.93")
        def publish(repository, packages, channel):
            self.assertEqual(repository, "fixture")
            self.assertEqual(channel, "stable" if "--channel" in arguments else "candidate")
            for path, evidence in packages:
                if channel == "stable":
                    self.assertIsNotNone(evidence)
                    self.assertEqual(evidence.read_text(), package_plugin.verify_package(path)["target"])
                else:
                    self.assertIsNone(evidence)
            return [{"target": package_plugin.verify_package(path)["target"]} for path, _ in packages]

        first_result = {"target": self.manifest["target"]}
        second_result = {"target": package_plugin.verify_package(second)["target"]}
        evidence = [self.root / "first.json", self.root / "second.json"]
        for path, result in zip(evidence, (first_result, second_result)):
            path.write_text(result["target"])
        both = {"publications": [first_result, second_result]}
        for arguments, expected in ((["--package", str(self.package)], first_result),
                                    (["--packages", str(self.package), str(second)], both),
                                    (["--channel", "stable", "--packages", str(self.package), str(second),
                                      "--evidence", str(evidence[0]), "--evidence", str(evidence[1])], both)):
            with patch.object(sys, "argv", ["publish", "--repository", "fixture", *arguments]), \
                    patch.object(publish_plugin, "publish_many", side_effect=publish), \
                    patch("builtins.print") as output:
                publish_plugin.main()
                self.assertEqual(json.loads(output.call_args.args[0]), expected)
        with patch.object(sys, "argv", ["publish", "--repository", "fixture", "--packages",
                                       str(self.package), str(second), "--evidence", "one.json"]), \
                patch.object(publish_plugin, "publish_many") as publish, \
                patch.object(sys, "stderr"):
            with self.assertRaises(SystemExit):
                publish_plugin.main()
            publish.assert_not_called()

    def test_package_hashes_bind_assets_binary_and_version(self):
        self.assertEqual(package_plugin.verify_package(self.package), self.manifest)
        (self.package / "bin/codex-session-board").write_bytes(b"other executable")
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)

    def test_portable_source_archive_preserves_the_complete_implementation_fingerprint(self):
        archive_path = self.package / "SOURCE.tar.gz"
        result = package_plugin.build_package(package_plugin.ROOT, archive_path)
        self.assertEqual(result["sha256"], hashlib.sha256(archive_path.read_bytes()).hexdigest())
        with tarfile.open(archive_path) as archive:
            names = archive.getnames()
            for name in (
                "src/main.rs", "tests/board_contract.rs", "acceptance/requirements.json",
                "acceptance/model.py", "acceptance/contract.lock.json", "Docs/MIGRATION.md",
                "scripts/dev", "scripts/dev_environment/configuration.py",
                "scripts/dev_environment/processes.py", "scripts/dev_environment/runtime.py",
            ):
                self.assertIn("codex-session-board/" + name, names)
            self.assertEqual(archive.getmember("codex-session-board/scripts/dev").mode & 0o777, 0o755)
            self.assertFalse(any(Path(name).suffix in {".sqlite3", ".db", ".log", ".png", ".pyc"} for name in names))
            archive.extractall(self.root / "relocated", filter="data")
        relocated = self.root / "relocated/codex-session-board"
        bindings = json.loads((relocated / "acceptance/bindings.json").read_text())
        self.assertEqual(
            package_plugin.fingerprint(relocated, bindings["fingerprint_paths"]),
            package_plugin.fingerprint(package_plugin.ROOT, bindings["fingerprint_paths"]),
        )
        self.assertEqual(package_plugin.versions(relocated), package_plugin.versions(package_plugin.ROOT))

    def test_extra_files_and_symlinks_cannot_enter_a_release(self):
        (self.package / "unexpected").write_text("runtime data")
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)
        (self.package / "unexpected").unlink()
        (self.package / "alias").symlink_to(self.package / "plugin.json")
        with self.assertRaisesRegex(ValueError, "symlinks"):
            package_plugin.verify_package(self.package)

    def test_source_preflight_checks_relocated_fingerprint_without_building_a_binary(self):
        with patch.object(package_plugin.subprocess, "run", side_effect=AssertionError("No build")):
            result = package_plugin.preflight(package_plugin.ROOT, self.root / "preflight")
        self.assertTrue(result["passed"])
        self.assertEqual(result["version"], package_plugin.versions(package_plugin.ROOT))
        self.assertTrue(Path(result["archive"]).is_file())
        self.assertEqual(len(result["fingerprint"]), 64)

    def test_source_preflight_detects_an_omitted_extensionless_entry(self):
        import package_migration
        for entry in ("scripts/validate", "scripts/release"):
            patterns = tuple(pattern for pattern in package_migration.PATTERNS if pattern != entry)
            with self.subTest(entry=entry), patch.object(package_migration, "PATTERNS", patterns):
                with self.assertRaisesRegex(ValueError, "missing implementation files"):
                    package_plugin.preflight(package_plugin.ROOT, self.root / Path(entry).name)

    def test_native_cache_metadata_is_excluded_only_for_installed_packages(self):
        metadata = self.package / ".codex-marketplace-install.json"
        metadata.write_text('{"source_type":"git"}')
        with self.assertRaisesRegex(ValueError, "hashes"):
            package_plugin.verify_package(self.package)
        self.assertEqual(package_plugin.verify_package(self.package, installed=True), self.manifest)
        (self.package / "bin/run-plugin").chmod(0o644)
        with self.assertRaisesRegex(ValueError, "executable"):
            package_plugin.verify_package(self.package, installed=True)

    def test_channel_versions_are_monotonic_and_bind_immutable_payloads(self):
        publish_plugin.validate_channel_progression(self.manifest, self.manifest)
        newer = {**self.manifest, "version": "0.2.94"}
        publish_plugin.validate_channel_progression(self.manifest, newer)
        with self.assertRaisesRegex(ValueError, "downgrade"):
            publish_plugin.validate_channel_progression(newer, self.manifest)
        with self.assertRaisesRegex(ValueError, "immutable"):
            publish_plugin.validate_channel_progression(
                self.manifest, {**self.manifest, "source_commit": "c" * 40})

    def test_source_fingerprint_changes_reject_publication(self):
        with patch.object(publish_plugin, "command", return_value="a" * 40):
            with patch.object(publish_plugin, "fingerprint", return_value="c" * 64):
                with self.assertRaisesRegex(ValueError, "stale"):
                    publish_plugin.authorize(self.package, "candidate", None, self.root)

    def test_stable_requires_full_gate_even_for_valid_candidate(self):
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with self.assertRaisesRegex(ValueError, "reviewed full"):
                    publish_plugin.authorize(self.package, "stable", None, self.root)
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with patch.object(publish_plugin, "verify", return_value=({"release_ready": False}, 2)):
                    with self.assertRaisesRegex(ValueError, "has not passed"):
                        publish_plugin.authorize(self.package, "stable", self.root / "evidence.json", self.root)
        with patch.object(publish_plugin, "command", side_effect=["a" * 40, ""]):
            with patch.object(publish_plugin, "fingerprint", return_value="b" * 64):
                with patch.object(publish_plugin, "verify", return_value=({"release_ready": True}, 0)):
                    self.assertEqual(
                        publish_plugin.authorize(self.package, "stable", self.root / "evidence.json", self.root),
                        self.manifest)
