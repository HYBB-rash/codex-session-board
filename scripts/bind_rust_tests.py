import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT))
from rust_adapter import executables, inventory
from acceptance.unittest_adapter import test_identifiers


def matching(names, *needles):
    return sorted(name for name in names if any(needle in name for needle in needles))


def build():
    names = inventory(executables())
    previous = json.loads((ROOT / "acceptance/bindings.json").read_text())
    group = lambda *needles: matching(names, *needles)
    archive = group("test::archive_flow::")
    state = group("::state_contract_tests::")
    runtime = group("::runtime_tests::")
    observer = group("test::observer_contract::")
    store = group("::store_tests::", "manual_save_", "test::column_sort::")
    isolation = group("::isolation_contract_tests::")
    title_store = group("::title_store_tests::")
    title_io = group("title_", "old_ack", "newer_state_before_write", "saved_state_reports")
    rpc = group("::rpc::tests::")
    configuration = group("::config::tests::")
    service = group("::service::tests::", "changed_config_or_port", "real_http_persistence", "service_update", "open_and_new")
    mcp = group("native_resource", "real_stdio_mcp", "saved_state_reports", "mcp_bridge_")
    security = group("host_origin", "navigation_is_canonical", "open_accepts", "open_and_new", "open_reports", "real_http_persistence", "real_http_pwa")
    suite = unittest.defaultTestLoader.discover(str(ROOT / "acceptance"), top_level_dir=str(ROOT))
    python_names = list(test_identifiers(suite))
    package_prefix = "acceptance.test_gate.PackageTests."
    python_gate = sorted(name for name in python_names if not name.startswith(package_prefix))
    mappings = {
        "STATE-001": state + archive,
        "AUTO-001": runtime + state + observer,
        "AUTO-002": runtime + state + observer,
        "AUTO-003": runtime + state + observer,
        "AUTO-004": runtime + store + isolation,
        "AUTO-005": runtime + state,
        "AUTO-006": runtime + store,
        "MANUAL-001": store + isolation + observer + archive + group("::sections::tests::"),
        "MANUAL-002": store + runtime + observer + archive,
        "MANUAL-003": store + runtime + observer + archive,
        "TITLE-001": title_io + isolation,
        "TITLE-002": title_io,
        "TITLE-003": title_io + title_store + isolation,
        "TITLE-004": title_io + title_store,
        "TITLE-005": title_store,
        "DISCOVERY-001": rpc + store + group("::projects::tests::"),
        "DISCOVERY-002": group("observer_selection") + store + observer,
        "NAV-001": security + group("test::singleton_guard::", "test::singleton_runtime::"),
        "APP-001": mcp,
        "UI-001": [],
        "UI-002": store,
        "UI-003": [],
        "MCP-001": mcp + archive,
        "DATA-001": configuration + group("open_and_new", "open_reports") + matching(
            python_names, "test_backup_preserves_committed_wal"),
        "DATA-002": service,
        "SERVICE-001": service + matching(python_names, "test_wrong_architecture_is_rejected"),
        "BOUNDARY-001": rpc + security + archive + group("disabled_title"),
        "SECURITY-001": security + store + archive,
        "HEALTH-001": observer,
        "DELIVERY-001": sorted(name for name in python_names if name.startswith(
            (package_prefix, "acceptance.test_delivery."))),
        "GATE-001": python_gate + group("example::project_check::", "test::git_hooks::"),
    }
    additions = {
        "STATE-001": group("test::legacy_mode::"),
        "DISCOVERY-001": group("::remote::", "::rpc::framing::"),
        "DATA-001": group("::remote::hosts::source::"),
        "DATA-002": group("::service::migration::"),
        "BOUNDARY-001": group("::rpc::", "::remote::", "::app::drafts::tests::", "test::legacy_mode::"),
    }
    for key, values in previous["requirements"].items():
        for name in values:
            if name.startswith(("lib::", "bin::", "test::", "example::")) and name not in names:
                raise ValueError("Previously bound native test disappeared: " + name)
        mappings[key].extend(values)
    for key, values in additions.items():
        mappings[key].extend(values)
    mappings = {key: list(dict.fromkeys(previous["requirements"][key] + values))
                for key, values in mappings.items()}
    bound = {name for values in mappings.values() for name in values}
    missing = set(names) - bound
    if missing:
        raise ValueError("Native tests without a requirement mapping: " + repr(sorted(missing)))
    bindings = {
        "schema_version": 1, "implementation": "Rust Codex session board",
        "adapter": {"command": ["{python}", "scripts/rust_adapter.py"], "timeout_seconds": 1800},
        "fingerprint_paths": [
            "Cargo.toml", "Cargo.lock", "build.rs", "clippy.toml", "project-rules.json", "src", "tests",
            "index.html", "config.example.json", "plugin.json", "mcp.json", ".mcp.json",
            ".codex-plugin", ".agents/plugins", ".githooks", ".github", "assets", "skills", "acceptance",
            "scripts", "examples", "AGENTS.md", "README.md", "Docs/*.md", "requirements-delivery.txt",
        ],
        "requirements": mappings,
    }
    bindings["fingerprint_paths"] = list(dict.fromkeys(
        previous["fingerprint_paths"] + bindings["fingerprint_paths"]
    ))
    path = ROOT / "acceptance/bindings.json"
    path.write_text(json.dumps(bindings, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"native_tests": len(names), "bound_tests": len(bound), "requirements": len(mappings)}))


if __name__ == "__main__":
    build()
