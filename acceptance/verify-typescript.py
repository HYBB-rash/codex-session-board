import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import model


ACCEPTANCE = Path(__file__).resolve().parent
ROOT = ACCEPTANCE.parent
BASELINE = "2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc"
FROZEN = {
    "model.py": "afc587078c415ccb631fed4efdcef304269096fbfece812e680a6d4e195738d7",
    "requirements.json": "5b20b6135b59c3daf4002bea17316c0fbaf80228cd822e24ffb9853a4d69c32b",
}
RUNTIME = [
    ["active", "inProgress", [], "in_progress"],
    ["systemError", "inProgress", [], "blocked"],
    ["idle", "failed", [], "blocked"],
    ["idle", "interrupted", [], "blocked"],
    ["idle", "completed", [], "review"],
    ["notLoaded", "", [], None],
    ["active", "completed", [], None],
    ["idle", "inProgress", [], None],
    ["active", "inProgress", ["waitingOnApproval"], "blocked"],
    ["active", "inProgress", ["waitingOnUserInput"], "blocked"],
]


def read_json(name):
    return json.loads((ACCEPTANCE / name).read_bytes())


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify_lock():
    lock = read_json("contract.lock.json")
    require(lock["schema_version"] == 1 and lock["files"] == FROZEN, "冻结文件锁与上游不同")
    for name, expected in FROZEN.items():
        require(sha256(ACCEPTANCE / name) == expected, f"冻结文件 {name} 已改变")
    contract = read_json("requirements.json")
    require(
        contract["contract_id"] == "codex-session-board/acceptance-v3",
        "冻结契约标识不匹配",
    )
    require(len(contract["requirements"]) == 31, "冻结契约不是 31 项")
    require(contract["states"] == [list(state) for state in model.STATE_LABELS], "状态名称或顺序不符")
    return contract


def snapshot(flow):
    return {field: getattr(flow, field) for field in ("state", "manual", "note", "version")}


def export_model():
    graph = model.reachable_paths()
    traces = []
    counts = {"actions": 0, "restarts": 0, "views": 0}
    require(len(graph) == 40 and len(set(model.EVENTS)) == 13, "冻结模型图规模不符")
    require(len({origin.key() for origin, _ in graph}) == 40, "冻结模型起点发生重复")
    for origin, path in graph:
        for event in model.EVENTS:
            current = model.Flow()
            steps = []
            for action in (*path, event):
                current = current.step(action)
                steps.append({"event": action, "expected": snapshot(current)})
                counts["actions"] += 1
                counts["restarts"] += action == "restart"
                counts["views"] += action == "view"
            traces.append({"path": path, "event": event, "origin": origin.key(), "steps": steps})
    require(len(traces) == 520, "冻结模型边数不符")
    require(counts == {"actions": 1417, "restarts": 40, "views": 40}, "冻结模型操作数不符")
    return {
        "states": model.STATE_LABELS,
        "runtime": RUNTIME,
        "initial": snapshot(model.Flow()),
        "traces": traces,
        **counts,
    }


def run_adapter(payload, node):
    with tempfile.TemporaryDirectory(prefix="board-acceptance-") as directory:
        fixture = Path(directory, "model.json")
        fixture.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
        execution = subprocess.run(
            [node, str(ACCEPTANCE / "typescript-adapter.mjs"), fixture.as_uri()],
            text=True, capture_output=True, cwd=ROOT, timeout=120, check=False,
        )
    require(execution.returncode == 0, execution.stderr.strip() or "TypeScript 适配器未完成")
    results = json.loads(execution.stdout)
    require(isinstance(results, list) and results, "适配器缺少结构化结果")
    ids = {row["id"] for row in results}
    require(len(ids) == len(results), "适配器结果重复")
    require(all(row["status"] in {"pass", "fail"} for row in results), "适配器结果状态无效")
    return [{"id": "frozen_requirements_and_model", "status": "pass"}, *results]


def fingerprint_of(paths):
    digest = hashlib.sha256()
    files = sorted({
        path
        for relative in paths
        for root in [ROOT / relative]
        for path in (root.rglob("*") if root.is_dir() else (root,))
        if path.is_file()
        and not {"node_modules", "__pycache__"}.intersection(path.relative_to(ROOT).parts)
    })
    require(files, f"缺少指纹输入 {paths}")
    for path in files:
        digest.update(str(path.relative_to(ROOT)).encode("utf-8"))
        digest.update(b"\x00")
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def requirement_results(contract, results):
    evidence = read_json("typescript-evidence.json")
    require(evidence["baseline"] == BASELINE, "逐项映射的上游提交不符")
    bindings = evidence["requirements"]
    require(
        list(bindings) == [item["id"] for item in contract["requirements"]],
        "逐项映射须按冻结契约覆盖所有 ID",
    )
    actual = {row["id"]: row for row in results}
    linked = {check for item in bindings.values() for check in item.get("checks", [])}
    require(linked == set(actual), "自动结果缺失、未绑定或出现未登记检查")
    rows = []
    for identifier, item in bindings.items():
        checks = [actual[check] for check in item.get("checks", [])]
        missing = item["missing"]
        require(missing and item["sources"], f"{identifier} 缺少取证范围或待验证原因")
        status = "fail" if any(check["status"] == "fail" for check in checks) else "pending"
        rows.append({"id": identifier, "status": status, **item, "results": checks})
    return rows


def main():
    parser = argparse.ArgumentParser(description="在隔离夹具中重放冻结契约，逐项报告取证缺口")
    parser.add_argument("--node", default="node")
    parser.add_argument("--model-only", action="store_true")
    args = parser.parse_args()
    try:
        contract = verify_lock()
        results = run_adapter(export_model(), args.node)
        source_fingerprint = fingerprint_of((
            "src", "ui", "scripts", "acceptance", ".codex-plugin", ".mcp.json",
            "README.md", "package.json", "package-lock.json", "tsconfig.json",
        ))
        runtime_fingerprint = fingerprint_of(("dist/extensions/session-board",))
        requirements = requirement_results(contract, results)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(json.dumps({"releaseReady": False, "error": str(error)}, ensure_ascii=False))
        return 2
    automated = all(row["status"] == "pass" for row in results)
    release_ready = automated and all(row["status"] == "pass" for row in requirements)
    report = {
        "contract": contract["contract_id"],
        "baseline": BASELINE,
        "sourceFingerprint": source_fingerprint,
        "runtimeFingerprint": runtime_fingerprint,
        "automatedModelPassed": automated,
        "releaseReady": release_ready,
        "checks": results,
    }
    if not args.model_only:
        report["requirements"] = requirements
    print(json.dumps(report, ensure_ascii=False, indent=2))
    passed = automated if args.model_only else release_ready
    return 0 if passed else 2


if __name__ == "__main__":
    sys.exit(main())
