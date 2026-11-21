from concurrent.futures import ThreadPoolExecutor
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import textwrap
import unittest
from unittest import mock

from scripts import checker_cache


FAKE_COMPILER = """#!/usr/bin/env python3
import hashlib,json,os,pathlib,sys,time
if pathlib.Path(sys.argv[0]).name == "rustc":
    print("rustc fake\\nhost: x86_64-unknown-linux-gnu")
elif "--version" in sys.argv:
    print("cargo fake")
else:
    time.sleep(0.15)
    arguments = sys.argv
    target = pathlib.Path(arguments[arguments.index("--target-dir")+1])
    manifest = pathlib.Path(arguments[arguments.index("--manifest-path")+1])
    source = manifest.parent.parent/"project_check.rs"
    if os.environ.get("CSB_FAKE_MUTATE_SNAPSHOT"):
        source.write_text("changed snapshot during build")
    code = source.read_bytes()
    binary = target/"project-check"
    binary.parent.mkdir(parents=True,exist_ok=True)
    binary.write_text("#!/usr/bin/env python3\\nprint("+repr(hashlib.sha256(code).hexdigest())+")\\n")
    binary.chmod(0o755)
    with open(os.environ["CSB_FAKE_BUILD_LOG"],"a") as stream:
        stream.write(str(target)+"\\n")
    if os.environ.get("CSB_FAKE_MUTATE_SOURCE"):
        pathlib.Path(os.environ["CSB_FAKE_MUTATE_SOURCE"]).write_text("changed during build")
    print(json.dumps({"target":{"name":"project-check"},"executable":str(binary)}))
"""


class CheckerCacheTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.source = self.fixture("first")
        tools = self.root / "bin"
        tools.mkdir()
        for name in ("cargo", "rustc"):
            path = tools / name
            path.write_text(textwrap.dedent(FAKE_COMPILER))
            path.chmod(0o755)
        self.log = self.root / "build.log"
        self.environment = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                                CSB_CHECKER_CACHE_DIR=str(self.root / "cache"),
                                CSB_FAKE_BUILD_LOG=str(self.log))

    def fixture(self, name):
        root = self.root / name
        (root / "scripts/checker").mkdir(parents=True)
        for filename, contents in (
            ("scripts/project_check.rs", "fn main() {}"),
            ("scripts/check", "checker launcher"),
            ("scripts/checker_cache.py", "cache implementation"),
            ("scripts/checker/Cargo.toml", "checker manifest"),
            ("scripts/checker/Cargo.lock", "locked dependencies"),
        ):
            (root / filename).write_text(contents)
        return root

    def resolve(self, source=None, **overrides):
        return checker_cache.resolve(source or self.source, dict(self.environment, **overrides))

    def test_cold_build_then_cross_worktree_hit_preserves_the_binary(self):
        first, built = self.resolve()
        second, cached = self.resolve(self.fixture("second"), CARGO_TARGET_DIR="/foreign/business")
        self.assertFalse(built["cache_hit"])
        self.assertTrue(cached["cache_hit"])
        self.assertEqual(first, second)
        self.assertEqual(built["binary_sha256"], cached["binary_sha256"])
        self.assertEqual(len(self.log.read_text().splitlines()), 1)
        self.assertNotIn("/foreign/business", self.log.read_text())

    def test_sources_lock_flags_configuration_and_compiler_changes_invalidate(self):
        _, previous = self.resolve()
        for filename in ("scripts/project_check.rs", "scripts/checker/Cargo.lock"):
            path = self.source / filename
            path.write_text(path.read_text() + "\nchanged")
            _, updated = self.resolve()
            self.assertFalse(updated["cache_hit"])
            self.assertNotEqual(previous["key"], updated["key"])
            previous = updated
        _, updated = self.resolve(RUSTFLAGS="-C opt-level=1")
        self.assertFalse(updated["cache_hit"])
        (self.source / ".cargo").mkdir()
        (self.source / ".cargo/config.toml").write_text("[build]\nrustflags=[]\n")
        _, configured = self.resolve()
        self.assertFalse(configured["cache_hit"])
        compiler = self.root / "bin/rustc"
        compiler.write_text(compiler.read_text() + "\n")
        _, replaced = self.resolve()
        self.assertFalse(replaced["cache_hit"])

    def test_corrupted_binary_or_record_is_rejected(self):
        binary, _ = self.resolve()
        binary.write_text("corrupted executable")
        with self.assertRaisesRegex(ValueError, "digest"):
            self.resolve()

    def test_same_identity_concurrent_requests_compile_once(self):
        with ThreadPoolExecutor(max_workers=2) as workers:
            futures = [workers.submit(self.resolve) for _ in range(2)]
            results = [future.result() for future in futures]
        self.assertEqual(results[0][0], results[1][0])
        self.assertEqual(len(self.log.read_text().splitlines()), 1)
        self.assertEqual(sorted(report["cache_hit"] for _, report in results), [False, True])

    def test_different_build_parameters_use_distinct_build_directories(self):
        with ThreadPoolExecutor(max_workers=2) as workers:
            futures = [workers.submit(self.resolve, RUSTFLAGS=value)
                       for value in ("-C opt-level=1", "-C opt-level=2")]
            results = [future.result() for future in futures]
        self.assertNotEqual(results[0][0], results[1][0])
        self.assertEqual(len(set(self.log.read_text().splitlines())), 2)

    def test_input_changes_during_build_cannot_publish_an_entry(self):
        with self.assertRaisesRegex(ValueError, "changed during compilation"):
            self.resolve(CSB_FAKE_MUTATE_SOURCE=str(self.source / "scripts/project_check.rs"))
        self.assertEqual(list((self.root / "cache/entries").iterdir()), [])

    def test_validation_wrappers_do_not_create_new_cache_identities(self):
        first, _ = self.resolve()
        wrappers = self.root / "report/tools"
        wrappers.mkdir(parents=True)
        for name in ("cargo", "rustc"):
            path = wrappers / name
            path.write_text('#!/usr/bin/env bash\nexec "' + str(self.root / "bin" / name) + '" "$@"\n')
            path.chmod(0o755)
        second, cached = self.resolve(PATH=str(wrappers) + os.pathsep + self.environment["PATH"])
        self.assertEqual(first, second)
        self.assertTrue(cached["cache_hit"])

    def test_missing_relative_path_entries_do_not_prevent_worktree_reuse(self):
        environment = self.environment["PATH"] + os.pathsep + "missing-tool-directory"
        first, _ = self.resolve(PATH=environment)
        second, report = self.resolve(self.fixture("other-root"), PATH=environment)
        self.assertEqual(first, second)
        self.assertTrue(report["cache_hit"])

    def test_configured_compiler_wrappers_and_linkers_content_changes_invalidate(self):
        configuration = self.source / ".cargo/config.toml"
        configuration.parent.mkdir()
        tool = self.root / "bin/config-tool"
        tool.write_text('#!/usr/bin/env bash\nexec "/bin/sh" "$@"\n')
        tool.chmod(0o755)
        for section, name in (
            ("build", "rustc"), ("build", "rustc-wrapper"),
            ("build", "rustc-workspace-wrapper"),
            ("target.x86_64-unknown-linux-gnu", "linker"),
            ("target.'cfg(unix)'", "linker"),
        ):
            with self.subTest(section=section, name=name):
                configuration.write_text(f"[{section}]\n{name}='../bin/config-tool'\n")
                contents = configuration.read_bytes()
                _, previous = self.resolve()
                tool.write_text(tool.read_text() + "\n")
                _, updated = self.resolve()
                self.assertFalse(updated["cache_hit"])
                self.assertNotEqual(previous["key"], updated["key"])
                self.assertEqual(configuration.read_bytes(), contents)

    def test_unsupported_config_tool_references_are_rejected(self):
        configuration = self.source / ".cargo/config.toml"
        configuration.parent.mkdir()
        for contents in (
            "include=['other.toml']\n",
            "[build]\nrustc-wrapper=['sh','wrapper']\n",
            "[build]\nrustflags=['-Clinker=unknown']\n",
            "[build]\nrustflags=['@external-flags']\n",
            "[env]\nRUSTC_WRAPPER='unknown'\n",
        ):
            with self.subTest(configuration=contents):
                configuration.write_text(contents)
                with self.assertRaises(ValueError):
                    self.resolve()
        configuration.write_text("")
        for name, value in (
            ("RUSTFLAGS", "-Clinker=untracked"),
            ("RUSTFLAGS", "@external-flags"),
            ("CARGO_ENCODED_RUSTFLAGS", "-C\x1flinker=untracked"),
            ("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", "-C link-arg=-fuse-ld=untracked"),
        ):
            with self.subTest(environment=name), self.assertRaises(ValueError):
                self.resolve(**{name: value})
        self.assertFalse(self.log.exists())

    def test_transient_source_change_during_copy_cannot_publish_a_snapshot(self):
        source = self.source / "scripts/project_check.rs"
        original = source.read_bytes()
        copyfile = checker_cache.shutil.copyfile

        def transient_copy(path, destination, *arguments, **keywords):
            if Path(path) != source:
                return copyfile(path, destination, *arguments, **keywords)
            source.write_text("transient wrong source")
            try:
                return copyfile(path, destination, *arguments, **keywords)
            finally:
                source.write_bytes(original)

        with mock.patch.object(checker_cache.shutil, "copyfile", side_effect=transient_copy):
            with self.assertRaisesRegex(ValueError, "snapshot"):
                self.resolve()
        self.assertEqual(source.read_bytes(), original)
        self.assertFalse(self.log.exists())
        self.assertEqual(list((self.root / "cache/entries").iterdir()), [])

    def test_snapshot_changes_during_compilation_cannot_publish_an_entry(self):
        source = self.source / "scripts/project_check.rs"
        original = source.read_bytes()
        with self.assertRaisesRegex(ValueError, "snapshot changed during compilation"):
            self.resolve(CSB_FAKE_MUTATE_SNAPSHOT="1")
        self.assertEqual(source.read_bytes(), original)
        self.assertEqual(list((self.root / "cache/entries").iterdir()), [])

    def test_non_object_cache_records_return_a_check_error(self):
        binary, _ = self.resolve()
        record = binary.parent / "CHECKER.json"
        for value in ([], None, 42, True, "record"):
            with self.subTest(record=value):
                record.write_text(json.dumps(value))
                with mock.patch.dict(os.environ, self.environment, clear=True), \
                        mock.patch.object(checker_cache.sys, "argv", [
                            "checker_cache.py", "--project", str(self.source), "--", "--help",
                        ]), mock.patch.object(checker_cache.subprocess, "call") as execute, \
                        contextlib.redirect_stderr(io.StringIO()) as error:
                    self.assertEqual(checker_cache.main(), 2)
                    execute.assert_not_called()
                    self.assertTrue(error.getvalue().strip())
