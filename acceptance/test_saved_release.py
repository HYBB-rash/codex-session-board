import hashlib
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import package_plugin
import package_scenarios
import release_plugin
import release_scenarios
import verify_package_startup


class SavedGitHub:
    def __init__(self):
        self.repository = "owner/plugin"
        self.assets = {}
        self.draft = True
        self.source = "a" * 40
        self.uploaded = []
        self.edits = []
        self.asset_ids = {}
        self.api_calls = []
        self.created = 0

    def release(self, tag):
        for name in self.assets:
            self.asset_ids.setdefault(name, len(self.asset_ids) + 1)
        return {"id": 7, "tag_name": tag, "draft": self.draft, "immutable": not self.draft,
                "upload_url": f"https://uploads.github.com/repos/{self.repository}/releases/7/assets{{?name,label}}", "assets": [
            {"id": self.asset_ids[name], "name": name, "digest": "sha256:" + hashlib.sha256(body).hexdigest()}
            for name, body in self.assets.items()
        ]}

    def upload(self, tag, path):
        self.uploaded.append(path.name)
        self.assets.setdefault(path.name, path.read_bytes())

    def download(self, tag, name, directory):
        result = directory / name
        result.write_bytes(self.assets[name])
        return result

    def api(self, path, method="GET", **fields):
        self.api_calls.append((method, path))
        if path.startswith("commits/"):
            return {"sha": self.source}
        if path == "git/ref/heads/main":
            return {"object": {"sha": self.source}}
        if path.startswith("compare/"):
            return {"status": "identical", "merge_base_commit": {"sha": self.source}}
        if path.startswith("git/ref/tags/"):
            return {}
        if path.startswith("releases/tags/"):
            return None
        if path.startswith("releases?"):
            return []
        if path == "releases" and method == "POST":
            self.created += 1
            return self.release(fields["tag_name"])
        if path == "releases/7":
            return self.release("v0.2.93")
        return {"enabled": True}

    def transfer(self, arguments, **options):
        assert arguments[:2] == ["gh", "api"]
        if "--input" in arguments:
            archive = Path(arguments[arguments.index("--input") + 1])
            assert arguments[2] == f"https://uploads.github.com/repos/{self.repository}/releases/7/assets?name={archive.name}"
            self.uploaded.append(archive.name)
            if archive.name in self.assets:
                return SimpleNamespace(returncode=1, stderr="already exists")
            self.assets[archive.name] = archive.read_bytes()
        else:
            assert "Accept: application/octet-stream" in arguments
            identifier = int(arguments[2].rsplit("/", 1)[1])
            name = next(name for name, asset_id in self.asset_ids.items() if asset_id == identifier)
            options["stdout"].write(self.assets[name])
        return SimpleNamespace(returncode=0, stderr="")

    def edit(self, tag, **fields):
        self.edits.append(fields)
        self.draft = fields.get("draft", self.draft)
        return self.release(tag)

    def gh(self, *arguments, **options):
        self.edits.append(arguments)
        if "--draft=false" in arguments:
            self.draft = False
        return SimpleNamespace(returncode=0)


class SavedReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.github = SavedGitHub()
        self.tag = "v0.2.93"
        self.target = "x86_64-unknown-linux-gnu"
        (self.root / "fixture").write_bytes(b"fixture")

    def archive(self, target=None, source=None):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            (package / "bin").mkdir()
            (package / ".codex-plugin").mkdir()
            for name in ("bin/codex-session-board", "bin/run-plugin"):
                (package / name).write_text("executable")
                (package / name).chmod(0o755)
            for name in ("plugin.json", ".codex-plugin/plugin.json"):
                (package / name).write_text('{"version":"0.2.93"}')
            files = package_plugin.hashes(package)
            (package / "RELEASE.json").write_text(json.dumps({
                "schema_version": 1, "target": target or self.target, "version": "0.2.93",
                "source_commit": source or self.github.source, "files": files,
                "build_sha256": "b" * 64,
                "binary_sha256": files["bin/codex-session-board"],
            }))
            output = io.BytesIO()
            with tarfile.open(fileobj=output, mode="w:gz") as contents:
                for path in package.iterdir():
                    contents.add(path, arcname=path.name)
            return output.getvalue()

    def startup(self, target=None):
        return {
            "schema_version": 1, "scope": "package_startup", "passed": True,
            "source_commit": self.github.source, "build_sha256": "b" * 64,
            "binary_sha256": hashlib.sha256(b"executable").hexdigest(),
            "version": "0.2.93", "target": target or self.target,
            "checks": verify_package_startup.CHECKS, "cleanup_confirmed": True,
        }

    def scenarios(self, manifest, body, startup):
        reports = {}
        cases = []
        for name, expected in release_scenarios.CASES.items():
            report = {**self.startup(manifest["target"]), "passed": expected is None}
            if expected:
                report.update(error=expected, checks=[])
            if name == "wrong-sidebar":
                report = {"passed": True, "cleanup_confirmed": True, "status": 403,
                          "bytes_per_connection": [0], "response": {"error": expected}}
            reports[name] = report
            cases.append({"name": name, "passed": True, "cleanup_confirmed": True,
                          "expected_rejection": expected is not None,
                          "actual_startup_passed": expected is None})
        return {
            "schema_version": 1, "scope": "pr_package_scenarios", "passed": True,
            "manifest": manifest, "fixture_sha256": hashlib.sha256(b"fixture").hexdigest(),
            "verifier_sha256": release_scenarios.digest(release_plugin.ROOT / "scripts/verify_package_startup.py"),
            "source_digests": release_scenarios.source_digests(release_plugin.ROOT),
            "package_asset_sha256": hashlib.sha256(body).hexdigest(),
            "startup_sha256": hashlib.sha256(startup).hexdigest(),
            "cases": cases, "reports": reports, "wrong_backend_bytes": [0],
        }

    def run_scenarios(self, project, package, fixture, output, connection=False):
        self.assertTrue(connection)
        output.mkdir()
        manifest = package_plugin.verify_package(package)
        body = self.github.assets[release_plugin.asset_name(manifest["target"])]
        startup = json.dumps(self.startup(manifest["target"])).encode()
        record = self.scenarios(manifest, body, startup)
        reports = record.pop("reports")
        for name, report in reports.items():
            (output / f"{name}.json").write_text(json.dumps(report))
        (output / "SCENARIOS.json").write_text(json.dumps(record))
        return 0

    def save_existing(self, target=None, startup=True, scenarios=True):
        target = target or self.target
        name = release_plugin.asset_name(target)
        body = self.archive(target)
        self.github.assets[name] = body
        self.github.assets[name + ".sha256"] = f"{hashlib.sha256(body).hexdigest()}  {name}\n".encode()
        if startup:
            saved_startup = json.dumps(self.startup(target)).encode()
            self.github.assets[release_plugin.startup_name(target)] = saved_startup
            if scenarios:
                with tarfile.open(fileobj=io.BytesIO(body)) as contents:
                    manifest = json.load(contents.extractfile("RELEASE.json"))
                self.github.assets[f"SCENARIOS-{target}.json"] = json.dumps(
                    self.scenarios(manifest, body, saved_startup)).encode()

    def test_scenarios_bind_all_cases_fixture_verifiers_and_actual_package(self):
        self.save_existing()
        archive = self.root / release_plugin.asset_name(self.target)
        archive.write_bytes(self.github.assets[archive.name])
        startup = self.root / release_plugin.startup_name(self.target)
        startup.write_bytes(self.github.assets[startup.name])
        fixture = self.root / "fixture"
        fixture.write_bytes(b"fixture")
        record = json.loads(self.github.assets[f"SCENARIOS-{self.target}.json"])
        manifest = record["manifest"]
        release_scenarios.validate(record, manifest, archive, startup, release_plugin.ROOT, fixture)
        changes = [
            lambda item: item.update(passed=False),
            lambda item: item.update(manifest={**manifest, "source_commit": "c" * 40}),
            lambda item: item.update(fixture_sha256="c" * 64),
            lambda item: item.update(verifier_sha256="c" * 64),
            lambda item: item.update(source_digests={}),
            lambda item: item.update(package_asset_sha256="c" * 64),
            lambda item: item.update(startup_sha256="c" * 64),
            lambda item: item["cases"].pop(),
            lambda item: item["cases"][1].update(name="success"),
            lambda item: item["cases"][1].update(expected_rejection=False),
            lambda item: item["cases"][1].update(cleanup_confirmed=False),
            lambda item: item["cases"][1].update(passed=False),
            lambda item: item["reports"]["mcp-config-drift"].update(error="another failure"),
            lambda item: item["reports"]["mcp-config-drift"].update(passed=True),
            lambda item: item["reports"]["mcp-config-drift"].update(source_commit="c" * 40),
            lambda item: item["reports"]["wrong-sidebar"].update(bytes_per_connection=[1]),
            lambda item: item["reports"]["wrong-sidebar"].update(status=200),
            lambda item: item["reports"]["success"].update(cleanup_confirmed=False),
            lambda item: item["reports"].pop("success"),
            lambda item: item.update(wrong_backend_bytes=[1]),
        ]
        for index, change in enumerate(changes):
            mutated = json.loads(json.dumps(record))
            change(mutated)
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, "scenario"):
                release_scenarios.validate(mutated, manifest, archive, startup, release_plugin.ROOT, fixture)

    def test_first_release_checks_tag_reference_before_resolving_commit(self):
        github = release_plugin.GitHub("owner/plugin")
        created = {"id": 7, "tag_name": self.tag, "draft": True, "assets": []}
        with patch.object(github, "api", side_effect=[None, {}, {"sha": self.github.source}, created]) as api, \
                patch.object(github, "release", return_value=None):
            self.assertEqual(github.ensure_draft(self.tag, self.github.source), created)
        self.assertEqual(github.release_ids, {self.tag: 7})
        self.assertEqual(api.call_args_list[0].args, (f"git/ref/tags/{self.tag}",))
        self.assertEqual(api.call_args_list[1].kwargs, {"ref": f"refs/tags/{self.tag}", "sha": self.github.source})
        self.assertEqual(api.call_args_list[-1].args, ("releases", "POST"))
        self.assertEqual(api.call_args_list[-1].kwargs, {
            "tag_name": self.tag, "target_commitish": self.github.source,
            "draft": True, "prerelease": True, "name": self.tag, "body": f"Source commit: {self.github.source}",
        })
        for result in (None, {}, [created], {"tag_name": "wrong", "draft": True}, {"tag_name": self.tag, "draft": False},
                       *({**created, "id": identifier} for identifier in (None, True, 0, "7"))):
            with patch.object(github, "api", side_effect=[{}, {"sha": self.github.source}, result]), \
                    patch.object(github, "release", return_value=None):
                with self.assertRaisesRegex(ValueError, "did not confirm"):
                    self.assertEqual(github.ensure_draft(self.tag, self.github.source), created)
                    self.assertEqual(github.release_ids, {self.tag: 7})
        with patch.object(release_plugin.subprocess, "run",
                          return_value=SimpleNamespace(returncode=0, stdout="{}", stderr="")) as run:
            github.api("releases/7", "PATCH", draft=False, make_latest="false")
        self.assertEqual(run.call_args.args[0][-4:], ["-F", "draft=false", "-f", "make_latest=false"])
        for discovered in (None, created):
            with patch.object(github, "api", side_effect=[{}, {"sha": self.github.source}, RuntimeError("403 denied")]), \
                    patch.object(github, "release", side_effect=[None, discovered]):
                if discovered is None:
                    with self.assertRaisesRegex(RuntimeError, "403 denied"):
                        github.ensure_draft(self.tag, self.github.source)
                else:
                    github.ensure_draft(self.tag, self.github.source)

    def test_draft_release_discovery_paginates_and_preserves_asset_digests(self):
        github = release_plugin.GitHub("owner/plugin")
        expected = {"id": 7, "tag_name": self.tag, "draft": True, "assets": [{"digest": "sha256:original"}]}
        with patch.object(github, "api", side_effect=[None, [{"tag_name": "v0.0.1"}], [expected]]) as api:
            self.assertEqual(github.release(self.tag), expected)
        self.assertEqual([call.args[0] for call in api.call_args_list], [
            f"releases/tags/{self.tag}", "releases?per_page=100&page=1", "releases?per_page=100&page=2",
        ])
        published = {**expected, "draft": False, "immutable": True, "assets": [{"digest": "sha256:updated"}]}
        with patch.object(github, "api", return_value=published) as api:
            self.assertEqual(github.release(self.tag), published)
            github.edit(self.tag, prerelease=False, make_latest="true")
        self.assertEqual([call.args[0] for call in api.call_args_list], ["releases/7"] * 3)
        self.assertEqual(api.call_args.args, ("releases/7", "PATCH"))
        with patch.object(github, "api", side_effect=[{}, {"sha": self.github.source}, None]) as api:
            with self.assertRaisesRegex(ValueError, "not readable"):
                github.ensure_draft(self.tag, self.github.source)
        self.assertEqual([call.args[0] for call in api.call_args_list], [
            f"git/ref/tags/{self.tag}", f"commits/{self.tag}", "releases/7"])
        with patch.object(github, "api", side_effect=RuntimeError("access denied")):
            with self.assertRaisesRegex(RuntimeError, "access denied"):
                github.release(self.tag)
        with patch.object(github, "api", return_value={**expected, "id": 8}):
            with self.assertRaisesRegex(ValueError, "release identity"):
                github.release(self.tag)
        github = release_plugin.GitHub("owner/plugin")
        with patch.object(github, "api", side_effect=[None, []]):
            self.assertIsNone(github.release(self.tag))
        with patch.object(github, "api", side_effect=[None, RuntimeError("access denied")]):
            with self.assertRaisesRegex(RuntimeError, "access denied"):
                github.release(self.tag)
        with patch.object(github, "api", return_value=expected) as api:
            self.assertEqual(github.release(self.tag), expected)
            api.assert_called_once_with(f"releases/tags/{self.tag}")

    def test_local_then_ci_reuses_exact_archive_without_building(self):
        self.save_existing()
        original = dict(self.github.assets)
        for attempt in range(2):
            directory = self.root / str(attempt)
            directory.mkdir()
            with patch.object(release_plugin, "build") as build, \
                    patch.object(release_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                    patch.object(verify_package_startup, "verify") as startup, \
                    patch.object(package_scenarios, "verify") as scenarios:
                report, fixture = self.root / f"startup-{attempt}.json", self.root / "fixture"
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, directory,
                                             report, fixture)
                build.assert_not_called()
                startup.assert_not_called()
                scenarios.assert_not_called()
                self.assertEqual(json.loads(report.read_text()), self.startup())
                self.assertEqual(package_plugin.verify_package(directory / "package")["source_commit"], self.github.source)
        self.assertEqual(self.github.assets, original)
        github = release_plugin.GitHub(self.github.repository)
        with patch.object(github, "api", side_effect=self.github.api), \
                patch.object(release_plugin.subprocess, "run", side_effect=self.github.transfer), \
                patch.object(release_plugin, "build") as build, patch.object(release_plugin, "authorize"):
            github.ensure_draft(self.tag, self.github.source)
            directory = self.root / "confirmed-id"
            directory.mkdir()
            release_plugin.save_platform(github, self.tag, self.target, self.github.source, directory)
            build.assert_not_called()
        self.assertEqual(self.github.created, 1)
        self.assertEqual(self.github.api_calls.count(("GET", f"releases/tags/{self.tag}")), 1)
        self.assertEqual(self.github.api_calls.count(("GET", "releases?per_page=100&page=1")), 1)
        self.assertEqual(self.github.assets, original)

    def test_startup_asset_is_uploaded_once_and_canonical_winner_is_read_back(self):
        self.save_existing(startup=False)
        report = self.root / "startup.json"
        with patch.object(release_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                patch.object(package_scenarios, "verify", side_effect=self.run_scenarios) as verify:
            directory = self.root / "saved"
            directory.mkdir()
            release_plugin.save_platform(self.github, self.tag, self.target, self.github.source,
                                         directory, report, self.root / "fixture")
            verify.assert_called_once()
        name = release_plugin.startup_name(self.target)
        self.assertEqual(report.read_bytes(), self.github.assets[name])
        self.assertEqual(self.github.uploaded, [name, release_plugin.scenarios_name(self.target)])
        self.assertEqual(json.loads((directory / name).read_text()), self.startup())
        self.assertTrue(report.with_suffix(".scenarios.json").is_file())

    def test_existing_startup_without_scenarios_runs_all_cases_and_preserves_startup(self):
        self.save_existing(scenarios=False)
        name = release_plugin.startup_name(self.target)
        self.github.assets[name] = json.dumps({**self.startup(), "started_at": "older"}).encode()
        previous = self.github.assets[name]
        report = self.root / "recovered/startup.json"
        directory = self.root / "recovered-saved"
        directory.mkdir()
        with patch.object(release_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                patch.object(package_scenarios, "verify", side_effect=self.run_scenarios) as verify:
            release_plugin.save_platform(self.github, self.tag, self.target, self.github.source,
                                         directory, report, self.root / "fixture")
            verify.assert_called_once()
        self.assertEqual(self.github.assets[name], previous)
        self.assertEqual(report.read_bytes(), previous)
        self.assertEqual(self.github.uploaded, [release_plugin.scenarios_name(self.target)])

    def test_failed_scenarios_keep_diagnostics_and_never_upload_success_proofs(self):
        self.save_existing(startup=False)
        report = self.root / "failed/startup.json"
        directory = self.root / "failed-saved"
        directory.mkdir()
        def failed(*arguments, **options):
            self.run_scenarios(*arguments, **options)
            return 1
        with patch.object(release_plugin, "authorize", side_effect=lambda package, *_: package_plugin.verify_package(package)), \
                patch.object(package_scenarios, "verify", side_effect=failed):
            with self.assertRaisesRegex(ValueError, "scenarios failed"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source,
                                             directory, report, self.root / "fixture")
        self.assertFalse(report.exists())
        self.assertTrue((report.with_suffix(".scenarios") / "SCENARIOS.json").is_file())
        self.assertEqual(self.github.uploaded, [])

    def test_missing_or_failed_scenarios_never_finalize_a_candidate(self):
        for target in package_plugin.TARGETS:
            self.save_existing(target)
        name = release_plugin.scenarios_name(self.target)
        original = json.loads(self.github.assets[name])
        variants = [None, json.dumps({**original, "passed": False}).encode()]
        for index, variant in enumerate(variants):
            if variant is None:
                self.github.assets.pop(name, None)
            else:
                self.github.assets[name] = variant
            directory = self.root / f"missing-scenarios-{index}"
            directory.mkdir()
            with patch.object(release_plugin, "source_checkout"), \
                    patch.object(release_plugin.subprocess, "run") as run:
                with self.assertRaisesRegex(ValueError, "scenarios"):
                    release_plugin.publish_saved(self.github, self.tag, self.github.source,
                                                 "candidate", None, directory)
                self.assertTrue(all(call.args[0][1] == "-c" for call in run.call_args_list))
            self.assertEqual(self.github.edits, [])
        self.github.assets[name] = json.dumps(original).encode()
        observed = self.github.release
        directory = self.root / "wrong-scenarios-digest"
        directory.mkdir()
        with patch.object(self.github, "release", side_effect=lambda tag: {
            **observed(tag), "assets": [
                {**asset, "digest": "sha256:wrong"} if asset["name"] == name else asset
                for asset in observed(tag)["assets"]
            ],
        }), patch.object(release_plugin, "source_checkout"), \
                patch.object(release_plugin.subprocess, "run"):
            with self.assertRaisesRegex(ValueError, "evidence asset digest"):
                release_plugin.publish_saved(self.github, self.tag, self.github.source,
                                             "candidate", None, directory)
        self.assertEqual(self.github.edits, [])

    def test_missing_or_mismatched_startup_evidence_never_publishes(self):
        for target in package_plugin.TARGETS:
            self.save_existing(target)
        name = release_plugin.startup_name(self.target)
        original = self.github.assets[name]
        variants = [None, b"[]"] + [
            json.dumps({**self.startup(), field: value}).encode()
            for field, value in (
                ("schema_version", 0), ("scope", "other"), ("passed", False),
                ("cleanup_confirmed", False), ("checks", []), ("source_commit", "c" * 40),
                ("build_sha256", "c" * 64), ("binary_sha256", "d" * 64),
                ("version", "0.2.94"), ("target", "other"),
            )
        ]
        for index, variant in enumerate(variants):
            with self.subTest(index=index):
                if variant is None:
                    self.github.assets.pop(name, None)
                else:
                    self.github.assets[name] = variant
                directory = self.root / str(index)
                directory.mkdir()
                def checkout(source, destination):
                    (destination / "scripts").mkdir(parents=True)
                with patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                        patch.object(release_plugin.subprocess, "run") as run:
                    with self.assertRaisesRegex(ValueError, "startup"):
                        release_plugin.publish_saved(self.github, self.tag, self.github.source,
                                                     "candidate", None, directory)
                    self.assertTrue(all(call.args[0][1] == "-c" for call in run.call_args_list))
                self.assertTrue(self.github.draft)
                self.assertEqual(self.github.edits, [])
        self.github.assets[name] = original
        observed = self.github.release
        with patch.object(self.github, "release", side_effect=lambda tag: {
            **observed(tag), "assets": [{**asset, "digest": "sha256:wrong"} for asset in observed(tag)["assets"]]
        }):
            with self.assertRaisesRegex(ValueError, "digest"):
                release_plugin.read_startup(self.github, self.tag, self.startup(), self.root)

    def test_missing_platform_stays_draft_without_channel_writes(self):
        self.save_existing()
        result = release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
        self.assertEqual(result["status"], "incomplete")
        self.assertTrue(self.github.draft)
        self.assertEqual(self.github.edits, [])

    def test_cli_incomplete_finalize_fails_and_automatic_candidate_dispatches_main(self):
        with patch.object(release_plugin, "GitHub", return_value=self.github), \
                patch.object(release_plugin, "command", side_effect=[self.github.source, ""]), \
                patch.object(release_plugin, "release_tag", return_value=self.tag), \
                patch.object(release_plugin, "publish_saved", return_value={"status": "incomplete"}), \
                patch.object(self.github, "ensure_draft", create=True), \
                patch.object(sys, "argv", ["release", "candidate", "--finalize", "--repository", "owner/plugin"]), \
                patch("builtins.print"):
            self.assertEqual(release_plugin.main(), 1)
        with patch.object(release_plugin, "GitHub", return_value=self.github), \
                patch.object(release_plugin, "command", side_effect=[self.github.source, ""]), \
                patch.object(release_plugin, "release_tag", return_value=self.tag), \
                patch.object(sys, "argv", ["release", "candidate", "--repository", "owner/plugin"]), \
                patch("builtins.print"):
            self.assertEqual(release_plugin.main(), 0)
        self.assertEqual(self.github.edits, [("workflow", "run", "plugin-release.yml", "--ref", "main")])
        self.assertEqual(self.github.api_calls, [])

    def test_unmerged_candidate_cannot_freeze_or_publish_channels(self):
        for target in package_plugin.TARGETS:
            self.save_existing(target)
        for comparison in (
            {"status": "diverged", "merge_base_commit": {"sha": self.github.source}},
            {"status": "ahead", "merge_base_commit": {"sha": "c" * 40}},
        ):
            with patch.object(self.github, "api", side_effect=[
                {"sha": self.github.source}, {"object": {"sha": "b" * 40}}, comparison,
            ]), patch.object(release_plugin, "source_checkout") as checkout, \
                    patch.object(release_plugin.subprocess, "run") as run:
                with self.assertRaisesRegex(ValueError, "contained in main"):
                    release_plugin.publish_saved(self.github, self.tag, self.github.source,
                                                 "candidate", None, self.root)
                checkout.assert_not_called()
                run.assert_not_called()
        self.assertEqual(self.github.edits, [])

    def test_actions_token_checks_immutability_before_channel_writes(self):
        for target in package_plugin.TARGETS:
            self.save_existing(target)
        original_api = self.github.api
        def api(path):
            if path == "immutable-releases":
                raise RuntimeError("Resource not accessible by integration (HTTP 403)")
            return original_api(path)
        def checkout(source, destination):
            (destination / "scripts").mkdir(parents=True)
        with patch.object(self.github, "api", side_effect=api), \
                patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin.subprocess, "run") as run:
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
        self.assertFalse(self.github.draft)
        self.assertEqual(run.call_count, 3)
        publication = run.call_args_list[-1].args[0]
        self.assertEqual(len(publication[publication.index("--packages") + 1:]), len(package_plugin.TARGETS))
        observed_release = self.github.release
        for draft, immutable in ((False, False), (True, True)):
            with patch.object(self.github, "release", side_effect=lambda tag: {
                **observed_release(tag), "draft": draft, "immutable": immutable,
            }), patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                    patch.object(release_plugin.subprocess, "run") as run:
                retry = self.root / f"mutable-{draft}"
                retry.mkdir()
                with self.assertRaisesRegex(ValueError, "must be immutable"):
                    release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, retry)
                self.assertEqual(run.call_count, 2)
        with patch.object(self.github, "release", return_value=None):
            with self.assertRaisesRegex(ValueError, "has not been saved"):
                release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)

    def test_mismatched_source_is_rejected_before_build_or_upload(self):
        self.github.assets[release_plugin.asset_name(self.target)] = self.archive(source="b" * 40)
        with patch.object(release_plugin, "build") as build:
            with self.assertRaisesRegex(ValueError, "different source"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, self.root)
            build.assert_not_called()
        self.assertEqual(self.github.uploaded, [])

    def test_upload_race_consumes_winner_without_replacing_it(self):
        winner = self.archive()

        def build(root, target, destination):
            archive = self.root / "fixture.tar.gz"
            archive.write_bytes(winner)
            release_plugin.unpack(archive, destination)
            self.github.assets[release_plugin.asset_name(target)] = winner
            return destination

        github = release_plugin.GitHub(self.github.repository)
        github.remember_release(self.tag, self.github.release(self.tag))
        with patch.object(release_plugin, "build", side_effect=build), patch.object(release_plugin, "authorize"), \
                patch.object(github, "api", side_effect=self.github.api), \
                patch.object(release_plugin.subprocess, "run", side_effect=self.github.transfer):
            release_plugin.save_platform(github, self.tag, self.target, self.github.source, self.root)
        self.assertEqual(self.github.assets[release_plugin.asset_name(self.target)], winner)
        self.assertIn(release_plugin.asset_name(self.target) + ".sha256", self.github.assets)
        self.assertTrue(all(path == "releases/7" for method, path in self.github.api_calls))

    def test_publish_retry_and_stable_keep_original_bytes(self):
        for target in release_plugin.TARGETS:
            self.save_existing(target)
        original = dict(self.github.assets)
        evidence = self.root / "evidence.json"
        evidence.write_text(json.dumps({"platforms": {target: "review.json" for target in release_plugin.TARGETS}}))
        (self.root / "review.json").write_text("{}")

        def checkout(source, directory):
            (directory / "scripts").mkdir(parents=True)

        with patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin.subprocess, "run") as run:
            run.side_effect = [SimpleNamespace(returncode=0), SimpleNamespace(returncode=0), RuntimeError("push interrupted")]
            with self.assertRaisesRegex(RuntimeError, "interrupted"):
                release_plugin.publish_saved(self.github, self.tag, self.github.source, "candidate", None, self.root)
            self.assertFalse(self.github.draft)
            for channel in ("candidate", "stable"):
                directory = self.root / channel
                directory.mkdir()
                run.side_effect = None
                release_plugin.publish_saved(self.github, self.tag, self.github.source, channel,
                                             evidence if channel == "stable" else None, directory)
        self.assertEqual(self.github.assets, original)
        self.assertEqual(self.github.uploaded, [])

    def test_stable_requires_evidence_and_complete_packages_before_writes(self):
        with self.assertRaisesRegex(ValueError, "reviewed evidence"):
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "stable", None, self.root)
        evidence = self.root / "evidence.json"
        evidence.write_text("{}")
        with self.assertRaisesRegex(ValueError, "missing required"):
            release_plugin.publish_saved(self.github, self.tag, self.github.source, "stable", evidence, self.root)
        self.assertEqual(self.github.edits, [])

    def test_rehearsal_release_verifies_both_packages_without_publishing_channels(self):
        for target in release_plugin.TARGETS:
            self.save_existing(target)
        tag = "rehearsal-" + self.github.source
        reference = "refs/heads/codex/rehearsal-main-fixture"

        def checkout(source, directory):
            (directory / "scripts").mkdir(parents=True)

        with patch.object(release_plugin, "source_checkout", side_effect=checkout), \
                patch.object(release_plugin, "release_tag", return_value=tag), \
                patch.object(release_plugin, "versions", return_value="0.2.93"), \
                patch.object(release_plugin.subprocess, "run") as run:
            result = release_plugin.publish_saved(
                self.github, tag, self.github.source, "candidate", None, self.root, reference)
        self.assertEqual(result["channel"], None)
        self.assertEqual(result["tag"], tag)
        self.assertFalse(self.github.draft)
        self.assertEqual(run.call_count, len(release_plugin.TARGETS))
        self.assertTrue(all(call.args[0][1] == "-c" for call in run.call_args_list))
        self.assertEqual(self.github.edits, [
            {"draft": False, "prerelease": True, "make_latest": "false"},
        ])

    def test_rehearsal_release_rejects_channel_promotion_and_wrong_source_tag(self):
        tag = "rehearsal-" + self.github.source
        reference = "refs/heads/codex/rehearsal-main-fixture"
        with patch.object(release_plugin, "release_tag", return_value=tag), \
                patch.object(release_plugin.subprocess, "run") as run:
            for selected_tag, channel, selected_reference in (
                (tag, "stable", reference),
                ("rehearsal-" + "b" * 40, "candidate", reference),
                (tag, "candidate", "refs/heads/main"),
            ):
                with self.subTest(tag=selected_tag, channel=channel):
                    with self.assertRaises(ValueError):
                        release_plugin.publish_saved(self.github, selected_tag, self.github.source,
                                                     channel, None, self.root, selected_reference)
            run.assert_not_called()
        self.assertEqual(self.github.edits, [])

    def test_archive_links_and_checksum_tampering_are_rejected(self):
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w:gz") as contents:
            entry = tarfile.TarInfo("link")
            entry.type = tarfile.SYMTYPE
            entry.linkname = "/etc/passwd"
            contents.addfile(entry)
        archive = self.root / "unsafe.tar.gz"
        archive.write_bytes(output.getvalue())
        with self.assertRaisesRegex(ValueError, "links"):
            release_plugin.unpack(archive, self.root / "unsafe")
        self.save_existing()
        self.github.assets[release_plugin.asset_name(self.target) + ".sha256"] = b"wrong"
        with patch.object(release_plugin, "authorize"):
            with self.assertRaisesRegex(ValueError, "checksum"):
                release_plugin.save_platform(self.github, self.tag, self.target, self.github.source, self.root)


if __name__ == "__main__":
    unittest.main()
