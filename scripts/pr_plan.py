import subprocess
from pathlib import Path


RISKS = ("connection", "package", "lifecycle")
PACKAGE_SCRIPTS = {
    "scripts/build-plugin", "scripts/run-plugin", "scripts/package_plugin.py",
    "scripts/package_migration.py", "scripts/release_plugin.py",
    "scripts/verify_package_startup.py", "scripts/package_scenarios.py",
}
CONNECTION_SCRIPTS = {
    "scripts/shared_desktop.py", "scripts/dev",
}
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


def plan(root, base, additions=()):
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
        if path == "README.md":
            continue
        relevant.add(path)
        if path.startswith(("src/rpc/", "src/config/", "src/app/", "src/remote/",
                            "examples/dev_fixture", "scripts/dev_environment/")) or path in {
                                "src/config.rs", "src/app.rs", "src/rpc.rs", *CONNECTION_SCRIPTS}:
            risks.update(("connection", "lifecycle"))
        if path in PACKAGE_SCRIPTS | {"plugin.json", ".codex-plugin/plugin.json", "mcp.json", ".mcp.json"} or path.startswith("src/") and any(
            part in path for part in ("mcp", "http", "service", "main.rs")
        ):
            risks.update(("package", "lifecycle"))
        if (path in TOOLS or path.startswith((".githooks/", ".github/"))
                or path.endswith(".rs") and path.startswith("scripts/")
                or not (path.startswith(("src/", "tests/", "examples/", "acceptance/"))
                        or path in UI | PACKAGE_SCRIPTS | CONNECTION_SCRIPTS
                        or path.startswith("scripts/dev_environment/"))):
            profile = "full"
    if profile != "full":
        if not relevant:
            profile = "records"
        elif relevant <= UI:
            profile = "ui"
    if "connection" in risks:
        risks.add("package")
    if profile == "full":
        risks.add("package")
    return {"base": base, "merge_base": merge_base, "paths": paths,
            "profile": profile, "risks": sorted(risks),
            "package_required": "package" in risks}


def groups(inventory, profile):
    available = {"::".join(name.split("::")[:2]) for name in inventory if "::" in name}
    if profile == "full":
        return sorted(available)
    if profile == "records":
        return []
    if profile == "ui":
        from scripts.integration_plan import UI_GROUPS
        selected = UI_GROUPS
    elif profile == "business":
        selected = available - {"test::git_hooks", "example::project_check"}
    else:
        raise ValueError("Unknown PR test profile")
    if not selected or not selected <= available:
        raise ValueError("Required PR test groups are missing")
    return sorted(selected)
