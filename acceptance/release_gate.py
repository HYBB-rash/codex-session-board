import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
import subprocess
import sys

from delivery_checks import DELIVERY_CHECKS
from release_identity import (
    BASELINE, PLATFORMS, contract_at, evidence_file, identify, json_file, need, parse, sha256,
)


ROOT = Path(__file__).resolve().parents[1]
MAX_AGE = timedelta(days=30)
STATUSES = {"pass", "fail", "error", "skip"}
ACCEPTANCE_TESTS = {
    "mcp-global-board": "the installed MCP plugin declares one global entrypoint for the complete board page",
    "mcp-shared-board-state": "MCP App and the web page read each other's saved board state",
    "mcp-real-revisions": "MCP saves use the live state and input revisions across HTTP",
    "mcp-inline-app-resources": "the MCP resource bundles its SDK and styles without linked scripts or frames",
    "manual-follow-up-recovery": "new-input notices survive incomplete observations and clear only on manual confirmation",
    "manual-change-history": "manual confirmations append durable change records and rejected saves leave them alone",
    "automatic-change-history": "confirmed automatic phases append their reasons once without replacing the saved note",
    "disabled-titles-preserve-save": "disabling title sync still saves state and never renames",
    "launcher-passes-arguments-directly": "the client launcher passes the argument array without a shell",
    "loopback-token-rejects-untrusted-writes": "only loopback and same-origin token-authorized writes reach the board",
}
TAP_TEST = re.compile(r"^(ok|not ok) \d+ - (.+)$")
TAP_DIRECTIVE = re.compile(r" # (SKIP|TODO)(?:\s.*)?$", re.IGNORECASE)


def text(value):
    return isinstance(value, str) and bool(value.strip())


def names(items):
    return [item["id"] for item in items]


def statuses(raw, expected):
    need(isinstance(raw, list), "缺少自动断言结果")
    result = {}
    for row in raw:
        name, status = row["id"], row["status"]
        need(name in expected and name not in result, f"未知或重复的自动断言 {name}")
        need(status in STATUSES, f"自动断言状态无效 {name}")
        result[name] = status
    return result


def bindings_at(root, requirements):
    bindings = json_file(root / "acceptance/gate-bindings.json")
    candidate = json_file(root / "acceptance/candidate-map.json")
    model = json_file(root / "acceptance/typescript-evidence.json")
    need(bindings["schemaVersion"] == 1 and bindings["baseline"] == BASELINE, "发布绑定版本错误")
    need(candidate["upstream_commit"] == BASELINE and model["baseline"] == BASELINE, "候选绑定的上游版本错误")
    require_ids = set(names(requirements))
    need(
        set(bindings["assertions"]) == set(candidate["requirements"]) == set(model["requirements"]) == require_ids,
        "发布绑定须覆盖冻结的全部 31 项需求",
    )
    model_ids = {check for item in model["requirements"].values() for check in item.get("checks", [])}
    node_tests = {identifier: item for identifier, item in candidate["evidence"].items() if item.get("test")}
    need(not (node_tests.keys() & ACCEPTANCE_TESTS.keys()), "独立与候选测试 ID 重名")
    node_tests.update({
        identifier: {"file": "acceptance/public-operations.test.mjs", "test": name}
        for identifier, name in ACCEPTANCE_TESTS.items()
    })
    providers = {"gate-tests", *model_ids, *node_tests, *DELIVERY_CHECKS}
    need(
        len(providers) == 1 + len(model_ids) + len(node_tests) + len(DELIVERY_CHECKS),
        "自动断言的提供方有重名",
    )
    for requirement in requirements:
        identifier = requirement["id"]
        assertions = bindings["assertions"][identifier]
        need(isinstance(assertions, list) and len(assertions) == len(requirement["then"]), f"{identifier} 未覆盖全部冻结断言")
        for binding in assertions:
            need(isinstance(binding, list) and all(text(check) for check in binding), f"{identifier} 自动断言绑定格式无效")
            need(len(binding) == len(set(binding)) and set(binding).issubset(providers), f"{identifier} 自动断言绑定未知或重复")
    return bindings["assertions"], model_ids, node_tests


def execute(root, command, timeout=180):
    return subprocess.run(command, cwd=root, text=True, capture_output=True, check=False, timeout=timeout)


def tap_results(output):
    for line in output.splitlines():
        match = TAP_TEST.fullmatch(line)
        if match:
            outcome, name = match.groups()
            directive = TAP_DIRECTIVE.search(name)
            if directive:
                yield name[:directive.start()], "skip"
            else:
                yield name, "pass" if outcome == "ok" else "fail"


def node_results(root, node, requested, tests):
    groups = {}
    for identifier in sorted(requested & tests.keys()):
        reference = tests[identifier]
        file = Path(reference["file"])
        if identifier in ACCEPTANCE_TESTS:
            compiled = root / file
        else:
            need(not file.is_absolute() and file.is_relative_to("src") and ".." not in file.parts, "Node 测试源码路径无效")
            compiled = root / "dist" / file.relative_to("src").with_suffix(".js")
        groups.setdefault(compiled, []).append((reference["test"], identifier))
    records = {}
    errors = []
    for path, required in groups.items():
        process = execute(root, [node, "--test", "--test-reporter=tap", str(path)])
        native = {}
        for test, status in tap_results(process.stdout):
            need(test not in native, f"重复的 Node 断言结果 {test}")
            native[test] = status
        for test, identifier in required:
            if test in native:
                records[identifier] = native[test]
        if process.returncode:
            errors.append(f"Node 测试失败 {path.relative_to(root)}")
    return records, errors


def delivery_results(identity):
    records = {}
    for identifier in DELIVERY_CHECKS:
        results = [
            item.get("deliveryChecks", {}).get(identifier)
            for item in identity["archives"].values()
        ]
        if any(results) or identity["archiveErrors"]:
            records[identifier] = "fail"
        elif len(results) == len(PLATFORMS) and all(result == "" for result in results):
            records[identifier] = "pass"
    return records


def automated_checks(root, node, bindings, model_ids, node_tests):
    requested = {name for assertions in bindings.values() for row in assertions for name in row}
    records = {}
    errors = []
    inventory = execute(root, [node, "acceptance/check-candidate.mjs"])
    if inventory.returncode:
        errors.append("候选清单无效 " + (inventory.stderr.strip() or inventory.stdout.strip())[-1000:])
    if requested & model_ids:
        command = [sys.executable, "-B", "acceptance/verify-typescript.py", "--model-only", "--node", node]
        process = execute(root, command)
        try:
            model = parse(process.stdout)
            if process.returncode:
                errors.append(model.get("error") or "冻结模型对照失败")
            records.update(statuses(model.get("checks"), model_ids))
        except (ValueError, KeyError, TypeError, AttributeError) as error:
            errors.append(f"冻结模型自动断言结果无效 {error}")
    fixtures, suite_errors = node_results(root, node, requested, node_tests)
    records.update(fixtures)
    errors.extend(suite_errors)
    if "gate-tests" in requested:
        command = [sys.executable, "-B", "-m", "unittest", "discover", "-s", "acceptance", "-p", "test_release_gate.py"]
        process = execute(root, command)
        ran = re.search(r"^Ran ([1-9][0-9]*) tests? in ", process.stderr, re.MULTILINE)
        records["gate-tests"] = "pass" if process.returncode == 0 and ran and "skipped=" not in process.stderr else "fail"
    return records, errors


def evidence_entries(evidence, identity, requirements):
    if evidence is None:
        return {}
    need(evidence["schemaVersion"] == 1, "现场验收凭证格式版本不符")
    need(evidence["contract"] == identity["contract"], "现场凭证的冻结契约或基线不符")
    need(evidence["sourceSha256"] == identity["candidate"]["sourceSha256"], "候选源码改变，旧现场凭证失效")
    live_requirements = {item["id"] for item in requirements if item["verification"] in {"mixed", "live"}}
    entries = {}
    need(isinstance(evidence["runs"], list), "现场凭证缺少逐项实测记录")
    for entry in evidence["runs"]:
        scope = entry["requirement"], entry["platform"]
        need(scope[0] in live_requirements and scope[1] in PLATFORMS, f"现场证据范围错误 {scope}")
        need(scope not in entries, f"现场记录重复 {scope}")
        entries[scope] = entry
    return entries


def live_scope(entry, platform, release, now):
    host = entry["host"]
    need((host["os"], host["arch"]) == PLATFORMS[platform], "实测宿主平台与归档范围不符")
    for field in ("machine", "codexBuild"):
        need(text(host[field]), f"缺少实测宿主 {field}")
    need(text(entry["operator"]), "缺少现场验收人")
    need(
        all(entry.get(field) == release[field] for field in ("version", "archiveSha256", "runtimeSha256")),
        "现场凭证的发行版本、归档或运行文件指纹不符",
    )
    timestamp = datetime.fromisoformat(entry["observedAt"].replace("Z", "+00:00"))
    need(timestamp.utcoffset() is not None and now - MAX_AGE <= timestamp <= now, "现场凭证已过期、未注明时区或来自未来")


def live_assertion(entry, root):
    status = entry.get("status")
    if status in {"fail", "error", "skip"}:
        return "fail", entry.get("observation") or f"现场断言 {status}"
    need(status == "pass" and text(entry.get("observation")), "现场断言缺少通过结果或观察记录")
    artifacts = entry["artifacts"]
    need(isinstance(artifacts, list) and artifacts, "现场断言缺少原始附件")
    paths = set()
    for item in artifacts:
        path = evidence_file(root, item["path"])
        need(path not in paths and path.stat().st_size > 0, f"现场附件重复或为空 {item['path']}")
        need(sha256(path) == item["sha256"], f"现场附件 SHA256 不一致 {item['path']}")
        paths.add(path)
    return "pass", entry["observation"]


def live_results(entry, platform, release, root, now, count):
    pending = lambda reason: [("pending", reason)] * count
    if not release:
        return pending("缺少与候选匹配的该平台发行归档")
    if entry is None:
        return pending("缺少该平台现场断言")
    try:
        live_scope(entry, platform, release, now)
        items = {}
        need(isinstance(entry["assertions"], list), "现场记录缺少逐条断言")
        for item in entry["assertions"]:
            index = item["index"]
            need(type(index) is int and 1 <= index <= count and index not in items, "现场断言次序错误或重复")
            items[index] = item
    except (ValueError, KeyError, TypeError, AttributeError) as error:
        return pending(str(error))
    result = []
    for index in range(1, count + 1):
        try:
            result.append(live_assertion(items[index], root) if index in items else ("pending", "缺少现场断言结果"))
        except (ValueError, KeyError, TypeError, AttributeError, OSError) as error:
            result.append(("pending", str(error)))
    return result


def aggregate(statuses):
    if "fail" in statuses:
        return "fail"
    return "pass" if statuses and all(status == "pass" for status in statuses) else "pending"


def requirement_result(requirement, bindings, results, live):
    assertions = []
    for index, clause in enumerate(requirement["then"], start=1):
        tests = [{"id": name, "status": results.get(name, "missing")} for name in bindings[index - 1]]
        statuses = ["pass" if result["status"] == "pass" else "fail" if result["status"] in STATUSES else "pending"
                    for result in tests]
        detail = ""
        if not tests and requirement["verification"] in {"automated", "mixed"}:
            detail = "缺少自动断言绑定"
            statuses.append("pending")
        if any(result["status"] == "missing" for result in tests):
            detail = "缺少自动断言结果"
        platforms = {platform: {"status": rows[index - 1][0], "detail": rows[index - 1][1]}
                     for platform, rows in live.items()}
        statuses.extend(result["status"] for result in platforms.values())
        assertions.append({
            "index": index, "assertion": clause, "status": aggregate(statuses),
            "automated": tests, "detail": detail, "live": platforms,
        })
    return {"id": requirement["id"], "title": requirement["title"],
            "status": aggregate([row["status"] for row in assertions]), "assertions": assertions}


def evaluate(requirements, bindings, results, identity, evidence, evidence_root, now, errors=()):
    global_errors = list(errors)
    try:
        entries = evidence_entries(evidence, identity, requirements)
    except (ValueError, KeyError, TypeError, AttributeError) as error:
        entries = {}
        global_errors.append(str(error))
    rows = []
    for requirement in requirements:
        identifier = requirement["id"]
        live = {}
        if requirement["verification"] in {"mixed", "live"}:
            for platform in PLATFORMS:
                live[platform] = live_results(
                    entries.get((identifier, platform)), platform, identity["archives"].get(platform),
                    evidence_root, now, len(requirement["then"]),
                )
        rows.append(requirement_result(requirement, bindings[identifier], results, live))
    complete = not global_errors and set(identity["archives"]) == PLATFORMS.keys() and all(
        row["status"] == "pass" for row in rows
    )
    return {
        "schemaVersion": 1,
        **identity,
        "releaseReady": complete,
        "errors": global_errors,
        "automated": results,
        "requirements": rows,
    }


def archive_arguments(values):
    archives = {}
    for value in values:
        platform, separator, path = value.partition("=")
        need(separator and path and platform in PLATFORMS and platform not in archives, "归档须为唯一的 平台=文件")
        archives[platform] = Path(path).absolute()
    return archives


def main(argv=None):
    parser = argparse.ArgumentParser(description="核验冻结契约、当前构建、双平台发行包和现场证据")
    parser.add_argument("--node", default="node")
    parser.add_argument("--archive", action="append", default=[], metavar="PLATFORM=FILE")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--identify", action="store_true")
    args = parser.parse_args(argv)
    try:
        requirements = contract_at(ROOT)
        bindings, model_ids, node_tests = bindings_at(ROOT, requirements)
        archives = archive_arguments(args.archive)
        identity = identify(ROOT, archives)
        if args.identify:
            print(json.dumps({"schemaVersion": 1, "releaseReady": False, **identity}, ensure_ascii=False, indent=2))
            return 2 if identity["archiveErrors"] else 0
        results, errors = automated_checks(ROOT, args.node, bindings, model_ids, node_tests)
        need(identify(ROOT, archives) == identity, "验证期间候选实现、运行文件或归档发生变化")
        results.update(delivery_results(identity))
        path = args.evidence.absolute() if args.evidence else None
        evidence = json_file(path) if path else None
        evidence_root = path.parent.resolve() if path else ROOT
        report = evaluate(
            requirements, bindings, results, identity, evidence, evidence_root,
            datetime.now(timezone.utc), errors,
        )
    except (OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.SubprocessError) as error:
        report = {"schemaVersion": 1, "releaseReady": False, "error": str(error)}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if report["releaseReady"]:
        return 0
    return 1 if report.get("error") or report.get("errors") or any(
        row["status"] == "fail" for row in report.get("requirements", [])
    ) else 2


if __name__ == "__main__":
    sys.exit(main())
