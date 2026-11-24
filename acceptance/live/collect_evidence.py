from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
from tempfile import NamedTemporaryFile, TemporaryDirectory

from candidate_receipt import candidate_identity
from release_gate import evidence_entries, live_results, live_scope
from release_identity import PLATFORMS, contract_at, evidence_file, json_file, need, sha256


TURNS = {
    "AUTO-001": (
        "trusted_approval_and_confirmed_resume",
        ("原生轮次开始后进入进行中，独立读回原生分区。", "可信批准后，同一轮确实继续执行。"),
    ),
    "AUTO-002": (
        "native_retry_preserved_running_and_note",
        ("原生批准等待进入已阻塞，显示等待原因。", "自动原因未覆盖人工备注。",
         "自动重试期间保持进行中。"),
    ),
    "AUTO-003": (
        "native_clarification_reviewed",
        ("澄清结束后进入待审阅，读回同轮完整原生答复。", "再次观察保持待审阅和原生分区。"),
    ),
}
TURN_EVENTS = {event for event, _ in TURNS.values()} | {
    "native_approval_wait_observed", "native_running_confirmed", "fixture_archived", "config_restored",
}


def timestamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def native_events(log, version):
    events = {}
    for line in log.read_text(encoding="utf-8").splitlines():
        if line.startswith("{"):
            row = json.loads(line)
            event = row.get("event")
            if row.get("version") == version and event in TURN_EVENTS:
                need(event not in events, f"同一次运行出现重复原生观察：{event}")
                events[event] = row
    need(events.keys() == TURN_EVENTS, "原生轮次缺少观察或清理回执")
    return events


def native_receipts(root, status_path, status, release):
    need(status["scenario"] == "native-turns" and status["mode"] == "copy"
         and status["status"] == "passed" and status["candidate"] == release["version"]
         and status["archiveSha256"] == release["archiveSha256"]
         and all(status.get(key) is True for key in ("archiveVerified", "cleanupVerified", "probePassed")),
         "Mac 原生轮次缺少同一归档的成功及清理回执")
    paths = [status_path, status_path.with_name("run.log")]
    need(status["runId"] == status_path.parent.name, "运行 ID 与记录目录不符")
    artifacts = []
    for path in paths:
        name = path.relative_to(root).as_posix()
        artifacts.append({"path": name, "sha256": sha256(evidence_file(root, name))})
    events = native_events(paths[-1], release["version"])
    approval = events["native_approval_wait_observed"]
    resume = events["trusted_approval_and_confirmed_resume"]
    retry = events["native_retry_preserved_running_and_note"]
    start = events["native_running_confirmed"]
    completed = events["native_clarification_reviewed"]
    restored = events["config_restored"]
    window = [timestamp(status["startedAt"]), *(timestamp(event["at"]) for event in events.values())]
    need(all(moment.utcoffset() is not None for moment in window) and window == sorted(window),
         "原生观察时间不属于同一次有序运行")
    need(start["state"] == "in_progress" and start["sameNativeSection"] is True
         and approval["reason"] and approval["notePreserved"] is True
         and approval["approvedTurn"] == resume["approvedTurn"]
         and all(resume[key] is True for key in ("actualCommandCompleted", "sameTurnResumed", "durableNote"))
         and retry["willRetry"] is True and retry["finalState"] == "review"
         and completed["boardState"] == "review" and start["clarifiedTurn"] == completed["clarifiedTurn"]
         and all(completed[key] is True for key in (
             "sameNativeSectionAfterObservation", "assistantReplyMatched", "notePreserved"))
         and "fixture_archived" in events
         and restored["exactBytes"] is True and restored["codexExactBytes"] is True, "原生轮次断言不完整")
    host = status.get("hostInfo") or {
        "os": "darwin", "arch": "arm64", "machine": status["host"], "codexBuild": status["codexBuild"],
    }
    return [
        {
            "requirement": requirement, "platform": "macos-arm64", "host": host,
            "operator": status.get("operator", "board-accept"), "observedAt": events[event]["at"],
            **{field: release[field] for field in ("version", "archiveSha256", "runtimeSha256")},
            "assertions": [
                {"index": index, "status": "pass", "observation": observation, "artifacts": artifacts}
                for index, observation in enumerate(observations, 1)
            ],
        } for requirement, (event, observations) in TURNS.items()
    ]


def merge(entries, incoming, conflicts):
    scope = incoming["requirement"], incoming["platform"]
    current = entries.get(scope)
    if current:
        old = {item["index"]: item["status"] for item in current["assertions"]}
        new = {item["index"]: item["status"] for item in incoming["assertions"]}
        differing = [index for index in old.keys() & new.keys() if old[index] != new[index]]
        if differing:
            conflicts.append({"requirement": scope[0], "platform": scope[1], "indexes": sorted(differing)})
            return
        if old.keys() > new.keys():
            return
        if not old.keys() <= new.keys():
            conflicts.append({"requirement": scope[0], "platform": scope[1],
                              "reason": "现场时间按需求记录，请提供覆盖两份断言的单次回执"})
            return
        if old.keys() == new.keys() and timestamp(incoming["observedAt"]) <= timestamp(current["observedAt"]):
            return
    entries[scope] = incoming


def collect(project, root, version, manifest, imports, gate):
    identity, archives = candidate_identity(root, version, manifest)
    requirements = contract_at(project)
    counts = {item["id"]: len(item["then"]) for item in requirements if item["verification"] in {"live", "mixed"}}
    now = datetime.now(timezone.utc)
    entries, conflicts, rejected = {}, [], []

    def accept(row, origin):
        requirement, platform = row["requirement"], row["platform"]
        try:
            release = identity["archives"][platform]
            live_scope(row, platform, release, now)
            assertions = row["assertions"]
            results = live_results(row, platform, release, root, now, counts[requirement])
            indexes = [item["index"] for item in assertions]
            need(indexes and len(set(indexes)) == len(indexes)
                 and all(type(index) is int and 1 <= index <= len(results) and results[index - 1][0] != "pending"
                         for index in indexes), "逐条断言、附件或摘要无效")
            merge(entries, row, conflicts)
        except (KeyError, ValueError, TypeError, AttributeError, OSError) as error:
            rejected.append({"source": str(origin), "requirement": requirement, "platform": platform,
                             "reason": str(error)})

    historical = root / f"live-{version}.json"
    for source in imports or ([historical] if historical.is_file() else []):
        for row in evidence_entries(json_file(source), identity, requirements).values():
            accept(row, source)
    for path in sorted((root / "cli-runs").glob("*/status.json")):
        try:
            status = json_file(path)
            if status.get("candidate") != version or status.get("status") != "passed":
                continue
            for row in native_receipts(root, path, status, identity["archives"]["macos-arm64"]):
                accept(row, path)
        except (KeyError, ValueError, TypeError, AttributeError, OSError) as error:
            rejected.append({"source": str(path), "reason": str(error)})
    missing, failed = [], []
    passed = 0
    for requirement, count in counts.items():
        for platform in sorted(PLATFORMS):
            assertions = entries.get((requirement, platform), {}).get("assertions", [])
            by_index = {item["index"]: item for item in assertions}
            for index in range(1, count + 1):
                item = by_index.get(index)
                scope = {"requirement": requirement, "platform": platform, "index": index}
                if item is None:
                    missing.append(scope)
                elif item["status"] == "pass":
                    passed += 1
                else:
                    failed.append({**scope, "status": item["status"], "observation": item.get("observation")})
    output = root / f"collected-{version}.json"
    report = {"candidate": version, "observations": len(entries), "passed": passed, "missing": missing, "failed": failed,
              "conflicts": conflicts, "rejected": rejected}
    if conflicts or rejected:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 1
    evidence = {"schemaVersion": 1, "contract": identity["contract"],
                "sourceSha256": identity["candidate"]["sourceSha256"],
                "runs": [entries[scope] for scope in sorted(entries)]}
    root.mkdir(parents=True, exist_ok=True)
    write_json(output, evidence)
    report["evidence"] = str(output)
    if gate:
        gate_path = root / f"collected-gate-{version}.json"
        code = run_gate(archives, output, gate_path)
        report["gate"] = str(gate_path)
    else:
        code = 1 if failed else 2 if missing else 0
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return code


def write_json(path, value):
    with NamedTemporaryFile(dir=path.parent, mode="w", encoding="utf-8", delete=False) as buffer:
        temporary = Path(buffer.name)
        try:
            json.dump(value, buffer, ensure_ascii=False, indent=2)
            buffer.write("\n")
            buffer.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def run_gate(archives, evidence, output):
    with TemporaryDirectory(prefix="board-accept-gate-") as temporary:
        with tarfile.open(archives["linux-x86_64"], "r:gz") as candidate:
            candidate.extractall(temporary, filter="data")
        script, = Path(temporary).glob("*/acceptance/release_gate.py")
        command = [sys.executable, "-B", str(script), "--evidence", str(evidence)]
        for platform, path in archives.items():
            command.extend(["--archive", f"{platform}={path}"])
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE, check=False)
        write_json(output, json.loads(result.stdout))
        return result.returncode
