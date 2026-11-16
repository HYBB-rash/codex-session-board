import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
UI_PATHS = {
    "index.html", "tests/ui_layout.rs", "scripts/check-ui-layout.mjs",
    "scripts/check-host-filter.mjs", "scripts/check-transfer-feedback.mjs",
    "Docs/PROJECT-FILTERS.md",
}
UI_GROUPS = {
    "lib::codex_session_board", "test::board_contract", "test::board_io",
    "test::plugin_runtime", "test::ui_layout",
}
TOOL_GROUPS = {"example::project_check", "test::git_hooks"}
VERSION_PATHS = {"Cargo.toml", "Cargo.lock", "plugin.json",
                 ".codex-plugin/plugin.json", "project-rules.json"}
PACKAGE_SCRIPTS = {
    "scripts/build-plugin", "scripts/run-plugin", "scripts/package_plugin.py",
    "scripts/package_migration.py", "scripts/release_plugin.py",
    "scripts/verify_package_startup.py", "scripts/package_scenarios.py",
}
CONNECTION_SCRIPTS = {
    "scripts/shared_desktop.py", "scripts/dev", "scripts/pr_connection_probe.py",
}
APPLICATION_SCRIPTS = PACKAGE_SCRIPTS | CONNECTION_SCRIPTS | {
    "scripts/check-app-bridge.mjs", "scripts/check-native-new.mjs",
    "scripts/check-native-target-source.mjs", "scripts/check-native-target-start.mjs",
    "scripts/check-native-target.mjs", "scripts/check-remote-catalog.mjs",
    "scripts/check-light-ui.mjs", "scripts/check-ui-layout.mjs",
    "scripts/check-host-filter.mjs", "scripts/check-transfer-feedback.mjs",
    "scripts/check-blocked-optional.mjs",
}
APPLICATION_UI = {
    "index.html", "assets/board.svg", ".codex-plugin/plugin.json",
}


def application_path(path):
    if path in {"tests/git_hooks.rs", "acceptance/unittest_adapter.py"}:
        return False
    return (
        path.endswith(".rs") and path.startswith(("src/", "tests/", "examples/dev_fixture/"))
        or path == "examples/dev_fixture.rs"
        or path.startswith("acceptance/") and path.endswith(".py")
        or path == "acceptance/bindings.json"
        or path.startswith(("scripts/native_new/", "scripts/dev_environment/"))
        and Path(path).suffix in (".py", ".js", ".mjs")
        or path in APPLICATION_SCRIPTS | APPLICATION_UI
    )


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def ancestor(older, newer):
    result = subprocess.run(["git", "merge-base", "--is-ancestor", older, newer], cwd=ROOT)
    if result.returncode not in (0, 1):
        raise ValueError("Cannot check integration ancestry")
    return result.returncode == 0


def document(reference, path):
    contents = git("show", f"{reference}:{path}")
    return tomllib.loads(contents) if path.endswith((".toml", ".lock")) else json.loads(contents)


def version_only(base, head, path):
    before, after = document(base, path), document(head, path)
    if path == "Cargo.toml":
        after["package"]["version"] = before["package"]["version"]
    elif path == "Cargo.lock":
        for package in after["package"]:
            if package["name"] == "codex-session-board":
                original = next(item for item in before["package"]
                                if item["name"] == package["name"])
                package["version"] = original["version"]
    elif path == "project-rules.json":
        previous = before["versioning"].get("feature_releases", [])
        releases = after["versioning"].get("feature_releases", [])
        if releases != previous + [{
            "after_commit": git("rev-parse", f"{head}^"),
            "from": document(base, "Cargo.toml")["package"]["version"],
            "to": document(head, "Cargo.toml")["package"]["version"],
            "decision": "冻结发布候选",
        }]:
            return False
        after["versioning"]["feature_releases"] = previous
    else:
        after["version"] = before["version"]
    return before == after


def changed(base, head):
    output = subprocess.check_output([
        "git", "diff", "--no-renames", "--name-only", "-z", base, head,
    ], cwd=ROOT, text=True)
    return set(filter(None, output.split("\0")))


def modes(reference, paths, root=None):
    records = subprocess.check_output(
        ["git", "ls-tree", "-rz", reference, "--", *sorted(paths)], cwd=root or ROOT, text=True)
    return {entry.split("\t", 1)[1]: entry.split("\t", 1)[0].split()[0]
            for entry in records.split("\0") if entry}


def context_path(path):
    return path.startswith("context/") and Path(path).suffix in (".md", ".yaml", ".json")


def document_path(path):
    return (path.startswith("Docs/") and Path(path).suffix in (".md", ".yaml", ".json", ".png", ".svg")
            or path == "README.md")


def plan(base, head="HEAD"):
    base, head = git("rev-parse", f"{base}^{{commit}}"), git("rev-parse", f"{head}^{{commit}}")
    if not ancestor(base, head):
        raise ValueError("Integration base must be an ancestor of the candidate")
    paths = changed(base, head)
    replay = None
    if git("ls-tree", head, "--", "context/integration-sources.json"):
        record = document(head, "context/integration-sources.json")
        if record.get("base") == base:
            replay = record
            ingress, replay_base = record["ingress"], record["replay_base"]
            parents = git("rev-list", "--parents", "-n", "1", ingress).split()
            if (len(parents) != 3 or parents[1:] != [replay_base, record["source_head"]]
                    or not ancestor(ingress, head) or ancestor(ingress, base)
                    or ancestor(record["source_head"], replay_base)):
                raise ValueError("Replay must identify a real new source merge")
            paths.update(changed(replay_base, ingress))
    profile = "full"
    before, after = modes(base, paths), modes(head, paths)
    ordinary = all(
        after.get(path, before.get(path)) == "100644"
        and before.get(path, after.get(path)) == "100644" for path in paths
    )
    if ordinary:
        relevant = {path for path in paths if not context_path(path) and not document_path(path)}
        try:
            relevant = {path for path in relevant
                        if path not in VERSION_PATHS or not version_only(base, head, path)}
        except (KeyError, ValueError, subprocess.CalledProcessError):
            return {"base": base, "profile": profile, "paths": sorted(paths), "replay": replay}
        existing = all(path in before and path in after for path in relevant)
        if relevant and relevant.issubset(UI_PATHS) and existing:
            profile = "ui"
        elif relevant and all(path in after and application_path(path) for path in relevant):
            profile = "application"
    return {"base": base, "profile": profile, "paths": sorted(paths), "replay": replay}


def selected(inventory, profile):
    if profile == "full":
        return set(inventory)
    groups = {"::".join(name.split("::")[:2]) for name in inventory if "::" in name}
    required = UI_GROUPS if profile == "ui" else groups - TOOL_GROUPS
    if profile not in ("ui", "business", "application") or not required or not required.issubset(groups):
        raise ValueError("Integration test plan has missing groups")
    return {name for name in inventory
            if profile == "application" and "::" not in name
            or any(name.startswith(prefix + "::") for prefix in required)}


def bound_inventory(bindings, platform):
    names = set()
    for mapping in bindings["requirements"].values():
        names.update(mapping["common"] + mapping.get(platform, [])
                     if isinstance(mapping, dict) else mapping)
    return names


def validate_report(report, expected_plan, source, inventory):
    chosen = selected(inventory, expected_plan["profile"])
    tests = report.get("tests", [])
    names = [test["id"] for test in tests]
    if (report.get("scope") != "integration" or report.get("plan") != expected_plan
            or report.get("source") != source or report.get("passed") is not True
            or report.get("release_ready") is not False
            or set(report.get("inventory", [])) != inventory
            or len(report.get("inventory", [])) != len(inventory)
            or report.get("selected") != sorted(chosen)
            or report.get("not_run") != sorted(inventory - chosen)
            or len(names) != len(chosen) or set(names) != chosen
            or any(test["status"] != "pass" for test in tests)):
        raise ValueError("Integration report does not match its complete inventory and plan")
