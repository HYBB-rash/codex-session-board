import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from rust_adapter import executables, inventory


def matching(names, *needles):
    return sorted(name for name in names if any(needle in name for needle in needles))


def build():
    names = inventory(executables())
    group = lambda *needles: matching(names, *needles)
    archive = group("test::archive_flow::")
    state = group("::state_contract_tests::")
    runtime = group("::runtime_tests::")
    observer = group("test::observer_contract::")
    store = group("::store_tests::", "manual_save_")
    isolation = group("::isolation_contract_tests::")
    title_store = group("::title_store_tests::")
    title_io = group("title_", "old_ack", "newer_state_before_write", "saved_state_reports")
    rpc = group("::rpc::tests::")
    configuration = group("::config::tests::")
    service = group("::service::tests::", "changed_config_or_port", "real_http_persistence", "service_update")
    mcp = group("native_resource", "real_stdio_mcp", "saved_state_reports", "mcp_bridge_")
    security = group("host_origin", "navigation_is_canonical", "open_accepts", "real_http_persistence", "real_http_pwa")
    python_gate = [
        "acceptance.test_gate.GateTests." + name
        for name in (
            "test_automatic_success_does_not_claim_release_completion",
            "test_missing_or_duplicate_bindings_and_results_cannot_pass",
            "test_skipped_checks_and_nonzero_adapter_exit_cannot_pass",
            "test_missing_test_selector_is_an_error_instead_of_an_empty_pass",
            "test_live_evidence_requires_current_build_and_valid_artifacts",
            "test_changed_contract_baseline_requires_an_explicit_update",
            "test_adapter_checks_the_target_instead_of_its_own_project",
            "test_report_cannot_overwrite_evidence_artifacts_or_verified_inputs",
        )
    ]
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
        "NAV-001": security,
        "APP-001": mcp,
        "UI-001": [],
        "UI-002": store,
        "UI-003": [],
        "MCP-001": mcp + archive,
        "DATA-001": configuration,
        "DATA-002": service,
        "SERVICE-001": service,
        "BOUNDARY-001": rpc + security + archive + group("disabled_title"),
        "SECURITY-001": security + store + archive,
        "HEALTH-001": observer,
        "DELIVERY-001": ["acceptance.test_gate.PackageTests.test_package_is_reproducible_and_has_only_portable_source"],
        "GATE-001": python_gate + group("example::project_check::", "test::git_hooks::"),
    }
    mappings = {key: sorted(set(values)) for key, values in mappings.items()}
    bound = {name for values in mappings.values() for name in values}
    missing = set(names) - bound
    if missing:
        raise ValueError("Native tests without a requirement mapping: " + repr(sorted(missing)))
    bindings = {
        "schema_version": 1, "implementation": "Rust Codex session board",
        "adapter": {"command": ["{python}", "scripts/rust_adapter.py"], "timeout_seconds": 600},
        "fingerprint_paths": [
            "Cargo.toml", "Cargo.lock", "clippy.toml", "project-rules.json", "src", "tests",
            "index.html", "config.example.json", "plugin.json", "mcp.json", ".mcp.json",
            ".codex-plugin", ".agents/plugins", ".githooks", "assets", "skills", "acceptance",
            "scripts", "AGENTS.md", "README.md", "Docs/*.md",
        ],
        "requirements": mappings,
    }
    path = ROOT / "acceptance/bindings.json"
    path.write_text(json.dumps(bindings, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"native_tests": len(names), "bound_tests": len(bound), "requirements": len(mappings)}))


if __name__ == "__main__":
    build()
