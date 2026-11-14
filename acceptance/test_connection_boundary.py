import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.package_migration import ROOT, package_contents


class ConnectionBoundaryTests(unittest.TestCase):
    def test_sibling_bypasses_fail_at_the_expected_sites_after_a_positive_build(self):
        with tempfile.TemporaryDirectory(prefix="board-boundary-") as temporary:
            root = Path(temporary).resolve()
            for name, content in package_contents(ROOT).items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            probes = []
            for entry, location in (("src/lib.rs", "src"), ("src/app.rs", "src/app")):
                library = root / entry
                library.write_text(library.read_text() + "\nmod connection_boundary_probe;\n")
                probe = root / location / "connection_boundary_probe.rs"
                probe.write_text(
                    "pub fn approved(app: &crate::app::App) {\n"
                    "let _ = app.connection(std::time::Duration::from_secs(1), crate::rpc::Role::Reader);\n"
                    "let _ = crate::preflight::sections;\n}\n")
                probes.append(probe)
            environment = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"))
            command = ["cargo", "check", "--lib", "--locked", "--offline", "--message-format=json"]

            def compile_probe():
                return subprocess.run(command, cwd=root, env=environment,
                                      text=True, capture_output=True, timeout=300)

            positive = compile_probe()
            self.assertEqual(positive.returncode, 0, positive.stdout + positive.stderr)
            cases = [
                ("alias", 'Wire::connect(path, timeout, role)', "E0599"),
                ("spacing", 'crate::rpc::DesktopConnection :: connect (path, timeout, role)', "E0599"),
                ("proxy", 'Wire::proxy(&[], timeout, role)', "E0599"),
                ("remote", 'Wire::remote(binding, timeout, role)', "E0624"),
                ("naked_config", 'Wire::configured(config, timeout, role)', "E0308"),
                ("forged_request", 'crate::app::ConnectionRequest(config)', "E0423"),
            ]
            lines = ["use crate::rpc::{DesktopConnection as Wire, Role};"]
            expected = {}
            for name, expression, code in cases:
                lines.extend([
                    f"fn {name}(config: &crate::config::Config, path: &std::path::Path,",
                    "binding: &crate::remote::hosts::source::Binding) {",
                    "let timeout = std::time::Duration::from_secs(1); let role = Role::Reader;",
                    f"let _ = {expression};", "}",
                ])
                for probe in probes:
                    expected[(str(probe.relative_to(root)), len(lines) - 1)] = code
            for probe in probes:
                probe.write_text("\n".join(lines) + "\n")
            negative = compile_probe()
            self.assertNotEqual(negative.returncode, 0, "业务模块绕过了连接边界")
            diagnostics = [json.loads(line) for line in negative.stdout.splitlines()]
            errors = [item["message"] for item in diagnostics
                      if item.get("reason") == "compiler-message"
                      and item["message"]["level"] == "error"]
            actual = {
                (str((root / span["file_name"]).relative_to(root)), span["line_start"]): error["code"]["code"]
                for error in errors if error.get("code")
                for span in error["spans"]
                if span["is_primary"] and root / span["file_name"] in probes
            }
            detail = "\n".join(error["rendered"] for error in errors)
            self.assertEqual(actual, expected, detail)
            self.assertEqual(len(errors), len(cases) * len(probes), detail)
