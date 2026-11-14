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
    return set(filter(None, git("diff", "--no-renames", "--name-only", "-z",
                               base, head).split("\0")))


def modes(reference, paths):
    records = git("ls-tree", "-r", reference, "--", *sorted(paths))
    return {line.split("\t", 1)[1]: line.split()[0] for line in records.splitlines()}


def context_path(path):
    return path.startswith("context/") and Path(path).suffix in (".md", ".yaml", ".json")


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
        relevant = {path for path in paths if not context_path(path)}
        try:
            relevant = {path for path in relevant
                        if path not in VERSION_PATHS or not version_only(base, head, path)}
        except (KeyError, ValueError, subprocess.CalledProcessError):
            return {"base": base, "profile": profile, "paths": sorted(paths), "replay": replay}
        existing = all(path in before and path in after for path in relevant)
        if relevant and relevant.issubset(UI_PATHS) and existing:
            profile = "ui"
        elif relevant and existing and all(
            path.endswith(".rs") and path.startswith(("src/", "tests/"))
            and path != "tests/git_hooks.rs" for path in relevant
        ):
            profile = "business"
    return {"base": base, "profile": profile, "paths": sorted(paths), "replay": replay}


def selected(inventory, profile):
    if profile == "full":
        return set(inventory)
    groups = {"::".join(name.split("::")[:2]) for name in inventory if "::" in name}
    required = UI_GROUPS if profile == "ui" else groups - TOOL_GROUPS
    if profile not in ("ui", "business") or not required or not required.issubset(groups):
        raise ValueError("Integration test plan has missing groups")
    return {name for name in inventory
            if any(name.startswith(prefix + "::") for prefix in required)}


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
