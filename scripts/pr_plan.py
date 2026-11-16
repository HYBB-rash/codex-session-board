import subprocess
from pathlib import Path

from scripts.integration_plan import (
    CONNECTION_SCRIPTS, PACKAGE_SCRIPTS, UI_GROUPS, application_path, document_path, modes,
)

RISKS = ("connection", "package", "lifecycle")
TOOLS = {
    "scripts/check", "scripts/validate", "scripts/validate_candidate.py",
    "scripts/pr_plan.py", "scripts/pr_delivery.py", "scripts/verify_pr_tests.py",
    "scripts/rust_adapter.py", "acceptance/unittest_adapter.py",
    "scripts/verify_acceptance.py", "scripts/verify_integration.py",
    "scripts/integration_plan.py", "project-rules.json", "clippy.toml", "AGENTS.md",
}
UI = {
    "index.html", "assets/board.svg", ".codex-plugin/plugin.json",
    "scripts/check-ui-layout.mjs", "scripts/check-host-filter.mjs",
    "scripts/check-transfer-feedback.mjs", "scripts/check-blocked-optional.mjs",
}


def git(root, *arguments):
    return subprocess.check_output(["git", *arguments], cwd=root, text=True).strip()


def changed(root, base):
    merge_base = git(root, "merge-base", base, "HEAD")
    paths = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", merge_base, "HEAD"],
        cwd=root, text=True,
    ).split("\0")[:-1]
    return merge_base, sorted(paths)


def regular_changes(root, base, paths):
    if not paths:
        return True
    before, after = modes(base, paths, root), modes("HEAD", paths, root)
    return all(after.get(path) == "100644" and before.get(path, "100644") == "100644"
               for path in paths)


def plan(root, base, additions=(), stage="ci"):
    if stage not in ("ci", "submit"):
        raise ValueError("Unknown PR validation stage")
    if set(additions) - set(RISKS):
        raise ValueError("Unknown PR risk; automatic checks cannot be removed")
    base = git(root, "rev-parse", f"{base}^{{commit}}")
    merge_base, paths = changed(root, base)
    risks = set(additions)
    profile = "business"
    relevant = set()
    for path in paths:
        if path.startswith(("context/", "Docs/")) and Path(path).suffix in (".md", ".yaml", ".json", ".png", ".svg"):
            continue
        if document_path(path):
            continue
        relevant.add(path)
        if path.startswith(("src/rpc/", "src/config/", "src/app/", "src/remote/",
                            "examples/dev_fixture", "scripts/dev_environment/",
                            "scripts/native_new/")) or path in {
                                "src/config.rs", "src/app.rs", "src/rpc.rs", *CONNECTION_SCRIPTS}:
            risks.update(("connection", "lifecycle"))
        if path in PACKAGE_SCRIPTS | {"plugin.json", ".codex-plugin/plugin.json", "mcp.json", ".mcp.json"} or path.startswith("src/") and any(
            part in path for part in ("mcp", "http", "service", "main.rs")
        ):
            risks.update(("package", "lifecycle"))
        if (path in TOOLS or path.startswith((".githooks/", ".github/"))
                or path.endswith(".rs") and path.startswith("scripts/")
                or not (application_path(path) or path in UI)):
            profile = "full"
    if not regular_changes(root, merge_base, relevant):
        profile = "full"
    if profile != "full":
        if not relevant:
            profile = "records"
        elif relevant <= UI:
            profile = "ui"
    if risks & {"connection", "lifecycle"}:
        risks.add("package")
    if profile == "full":
        risks.add("package")
    strategy = {"base": base, "merge_base": merge_base, "paths": paths,
                "profile": profile, "risks": sorted(risks),
                "package_stage": "release"}
    if stage == "submit":
        strategy["stage"] = stage
    return strategy


def submit_groups(available, paths):
    if any(path.startswith(("tests/support/", "examples/dev_fixture")) for path in paths):
        return available - {"test::git_hooks", "example::project_check"}
    selected = set()
    if any(path.startswith("src/") for path in paths):
        selected.update(group for group in available if group.startswith("lib::"))
        selected.add("test::board_contract")
    for path in paths:
        if path.startswith("tests/") and path.endswith(".rs"):
            selected.add("test::" + path.split("/")[1].split(".")[0])
    if any(path == "index.html" or path.endswith(".mjs") for path in paths):
        selected.update(UI_GROUPS)
    if not selected <= available:
        raise ValueError("Required local PR test groups are missing")
    return selected


def groups(inventory, profile, paths=(), stage="ci"):
    if stage not in ("ci", "submit"):
        raise ValueError("Unknown PR validation stage")
    available = {"::".join(name.split("::")[:2]) for name in inventory if "::" in name}
    if profile == "full":
        return sorted(available)
    if profile == "records":
        return []
    if profile == "business" and stage == "submit":
        return sorted(submit_groups(available, paths))
    if profile == "ui":
        selected = UI_GROUPS
    elif profile == "business":
        selected = available - {"test::git_hooks", "example::project_check"}
    else:
        raise ValueError("Unknown PR test profile")
    if not selected or not selected <= available:
        raise ValueError("Required PR test groups are missing")
    return sorted(selected)


def validate_report(report, strategy, source, inventory):
    from scripts.verify_acceptance import validate_results
    prefixes = groups(inventory, strategy["profile"], strategy.get("paths", ()),
                      strategy.get("stage", "ci"))
    chosen = {name for name in inventory if "::" not in name
              or any(name.startswith(prefix + "::") for prefix in prefixes)}
    outcomes = validate_results(report, chosen)
    if (not chosen or report.get("scope") != "pr_tests" or report.get("source") != source
            or report.get("plan") != strategy or report.get("passed") is not True
            or report.get("release_ready") is not False
            or report.get("inventory") != sorted(inventory)
            or report.get("selected") != sorted(chosen)
            or report.get("not_run") != sorted(inventory - chosen)
            or any(item["status"] != "pass" for item in outcomes.values())):
        raise ValueError("PR report does not prove the complete selected test plan")
