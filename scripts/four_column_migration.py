import argparse
import fcntl
import json
import re
import sys
from pathlib import Path
from uuid import UUID

try:
    from scripts.records import write_record
except ModuleNotFoundError:
    from records import write_record


STATES = {"in_progress", "review", "blocked", "to_archive"}
CARD_GUARDS = ("inputVersion", "turnRevision", "metadataSha256")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def section_id(value):
    require(isinstance(value, str) and str(UUID(value)) == value, "分区或会话 ID 无效")
    return value


def digest(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), "SHA256 无效")
    return value


def version(value):
    require(type(value) is int and value >= 0, "版本或读回时刻无效")
    return value


def card_key(row):
    return row["hostId"], section_id(row["threadId"])


def card_index(snapshot):
    rows = {card_key(row): row for row in snapshot["cards"]}
    require(len(rows) == len(snapshot["cards"]), "读回中存在重复会话")
    return rows


def membership(snapshot, hosts, old):
    require(set(snapshot["hosts"]) == set(hosts), "原生主机读回范围改变")
    members = set()
    for host_id, binding in hosts.items():
        observed = snapshot["hosts"][host_id]
        require(observed["source"] == binding["source"], f"{host_id} 来源身份改变")
        require(observed["complete"] is True, f"{host_id} 原生目录读回不完整")
        require(observed["backendId"] == binding["backendId"], f"{host_id} 旧后台映射改变")
        require(observed["bindings"] == binding["bindings"], f"{host_id} 保留分区映射改变")
        for thread_id in observed["oldMembers"]:
            key = host_id, section_id(thread_id)
            require(key not in members, "旧分区读回中存在重复会话")
            members.add(key)
    logical = [card_key(row) for row in snapshot["oldMembers"]]
    require(len(logical) == len(set(logical)) and set(logical) == members, "侧栏与后台成员不一致")
    require(snapshot["oldSectionId"] == old, "旧逻辑分区改变")
    return logical


def validate_bindings(bindings, old):
    require(set(bindings) == STATES, "保留分区映射必须恰含四个状态")
    targets = {section_id(target) for target in bindings.values()}
    require(len(targets) == len(STATES) and old not in targets, "目标分区 ID 重复或包含旧分区")


def validate_card(row):
    require(row["archived"] is False and row["pinned"] is False and row["running"] is False,
            "会话正在执行、已归档或已置顶")
    version(row["version"])
    version(row["inputVersion"])
    require(isinstance(row["turnRevision"], str) and row["turnRevision"], "缺少轮次版本")
    require(isinstance(row["positionRevision"], str) and row["positionRevision"], "缺少位置版本")
    digest(row["metadataSha256"])
    if row["sectionId"] is not None:
        section_id(row["sectionId"])


def start(plan, snapshot):
    old = section_id(plan["oldSectionId"])
    hosts = plan["hosts"]
    digest(plan["candidateSha256"])
    digest(plan["backupSha256"])
    validate_bindings(plan["bindings"], old)
    require(snapshot["bindings"] == plan["bindings"], "保留逻辑分区映射改变")
    require("local" in hosts, "缺少本机分区读回")
    for binding in hosts.values():
        digest(binding["source"])
        validate_bindings(binding["bindings"], section_id(binding["backendId"]))
    require(snapshot["accountId"] == plan["accountId"] and plan["accountId"], "账号不匹配")
    require(snapshot["oldPresent"] is True, "未确认旧分区仍存在")
    require(all(host["oldPresent"] is True for host in snapshot["hosts"].values()),
            "未确认各主机的旧后台分区仍存在")
    planned = {card_key(row): row["state"] for row in plan["moves"]}
    ordered = membership(snapshot, hosts, old)
    require(len(planned) == len(plan["moves"]) and set(planned) == set(ordered),
            "计划与实时旧分区成员不一致")
    rows = card_index(snapshot)
    moves = []
    for key in ordered:
        before = rows[key]
        validate_card(before)
        require(before["sectionId"] == old, "会话已离开旧分区")
        state = planned[key]
        require(state is None or state in STATES, "目标须为四个状态之一或原生默认区")
        target = None if state is None else plan["bindings"][state]
        moves.append({"before": before, "target": target, "after": None})
    return {"phase": "moving", "plan": plan, "moves": moves, "pending": None,
            "observedAt": version(snapshot["observedAt"])}


def check_snapshot(ledger, snapshot):
    plan = ledger["plan"]
    require(snapshot["accountId"] == plan["accountId"], "账号改变")
    require(snapshot["bindings"] == plan["bindings"], "保留逻辑分区映射改变")
    require(version(snapshot["observedAt"]) > ledger["observedAt"], "须提交更新的独立读回")
    ordered = membership(snapshot, plan["hosts"], plan["oldSectionId"])
    rows = card_index(snapshot)
    known = [card_key(move["before"]) for move in ledger["moves"]]
    members = set(ordered)
    require(members <= set(known), "出现不在迁移计划中的旧分区成员")
    for host_id in plan["hosts"]:
        expected = [key for key in known if key[0] == host_id and key in members]
        require([key for key in ordered if key[0] == host_id] == expected,
                f"{host_id} 的旧分区成员顺序已改变")
    require(all((rows[key]["sectionId"] == plan["oldSectionId"]) == (key in members) for key in known),
            "卡片位置与旧分区成员读回不一致")
    return rows, members


def matches_target(before, row, target):
    return (
        row["sectionId"] == target and row["version"] >= before["version"]
        and row["positionRevision"] != before["positionRevision"]
        and all(row[field] == before[field] for field in CARD_GUARDS)
    )


def move_args(row, target, before_thread=None):
    result = {"threadId": row["threadId"], "hostId": row["hostId"], "sectionId": target,
              "source": "codex"}
    if before_thread is not None:
        result["beforeThreadId"] = before_thread
    return result


def reconcile_move(ledger, rows, index, rollback, retry):
    move = ledger["moves"][index]
    before = move["after"] if rollback else move["before"]
    current = rows[card_key(before)]
    validate_card(current)
    target = move["before"]["sectionId"] if rollback else move["target"]
    if matches_target(before, current, target):
        require(ledger["pending"] == index, "没有已记录的操作意图，须重新核对会话")
        move["after"] = None if rollback else current
        ledger["pending"] = None
        return {"action": "confirmed", "threadId": current["threadId"]}
    require(current == before, f"会话位置或版本已改变：{current['threadId']}")
    require(ledger["pending"] is None or retry, "回执未确认，先核实原生写入已停止再显式重试")
    next_thread = None
    if rollback:
        candidates = (other["before"] for other in ledger["moves"][index + 1:]
                      if other["before"]["hostId"] == before["hostId"] and other["after"] is None)
        next_row = next(candidates, None)
        if next_row is not None:
            guard = rows[card_key(next_row)]
            require(guard["sectionId"] == target, "恢复顺序所需的相邻会话已移走")
            next_thread = next_row["threadId"]
    ledger["pending"] = index
    return {"action": "move", "arguments": move_args(current, target, next_thread)}


def advance(ledger, snapshot, retry=False, rollback=False, delete=False):
    rows, members = check_snapshot(ledger, snapshot)
    phase = ledger["phase"]
    pending = ledger["pending"]
    if phase == "deleted":
        raise ValueError("旧分区已删除，恢复时须先重建并重新绑定新 ID")
    if phase == "rolled_back":
        raise ValueError("本次迁移已回滚，继续迁移须用新计划和账本")
    if phase == "deleting" and snapshot["oldPresent"] is False:
        require(not rollback, "旧分区已删除，恢复时须先重建并重新绑定新 ID")
        require(not members and all(host["oldPresent"] is False for host in snapshot["hosts"].values()),
                "仍有旧后台分区，须人工核对")
        ledger.update(phase="deleted", pending=None)
        result = {"action": "deleted"}
    else:
        require(snapshot["oldPresent"] is True
                and all(host["oldPresent"] is True for host in snapshot["hosts"].values()),
                "旧分区未在所有原生来源读回")
        if phase == "deleting":
            require(retry, "删除回执未确认，先核实原生操作已停止再显式重试")
        elif retry:
            require(pending is not None, "没有可重试的移动")
        if rollback and phase == "moving":
            require(pending is None, "请先核实未确认的移动")
            phase = "rollback"
        reversing = phase == "rollback"
        remaining = [index for index, move in enumerate(ledger["moves"])
                     if (move["after"] is not None) == reversing]
        require(not delete or (phase == "moving" and not remaining), "删除前须完成全部移动")
        if remaining:
            index = pending if pending is not None else (remaining[-1] if reversing else remaining[0])
            result = reconcile_move(ledger, rows, index, reversing, retry)
            ledger["phase"] = phase
        elif reversing:
            ledger["phase"] = "rolled_back"
            result = {"action": "rolled_back"}
        else:
            require(not members, "旧分区仍有成员")
            require(all(rows[card_key(move["before"])] == move["after"] for move in ledger["moves"]),
                    "迁移后会话再次改变，须重新核对")
            require(snapshot["fourColumnCandidate"] == ledger["plan"]["candidateSha256"]
                    and snapshot["oldTargetRejected"] is True, "缺少已切换四列及旧写入拒绝的证据")
            if delete or phase == "deleting":
                ledger["phase"] = "deleting"
                result = {"action": "delete",
                          "arguments": {"sectionId": ledger["plan"]["oldSectionId"]}}
            else:
                result = {"action": "ready_to_delete"}
    ledger["observedAt"] = snapshot["observedAt"]
    return result


def main():
    parser = argparse.ArgumentParser(description="读取并核对四列迁移账本，输出下一项原生操作")
    parser.add_argument("ledger", type=Path)
    parser.add_argument("snapshot", type=Path)
    operation = parser.add_mutually_exclusive_group()
    operation.add_argument("--plan", type=Path)
    operation.add_argument("--retry", action="store_true")
    operation.add_argument("--rollback", action="store_true")
    operation.add_argument("--delete", action="store_true")
    arguments = parser.parse_args()
    ledger = arguments.ledger
    try:
        lock_path = ledger.with_name(ledger.name + ".lock")
        inputs = {path.resolve() for path in (arguments.snapshot, arguments.plan) if path is not None}
        require(ledger.resolve() not in inputs and lock_path.resolve() not in inputs,
                "账本及锁不能与计划或读回使用同一文件")
        with lock_path.open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            snapshot = json.loads(arguments.snapshot.read_text())
            if arguments.plan:
                require(not ledger.exists(), "迁移账本已存在")
                record = start(json.loads(arguments.plan.read_text()), snapshot)
                result = {"action": "planned", "cards": len(record["moves"])}
            else:
                record = json.loads(ledger.read_text())
                result = advance(record, snapshot, arguments.retry, arguments.rollback, arguments.delete)
            write_record(ledger, record)
        print(json.dumps(result, ensure_ascii=False))
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"迁移停止：{error}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
