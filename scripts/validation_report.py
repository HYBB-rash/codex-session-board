from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import uuid
from scripts.records import write_record as write


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def create(output, root, scope):
    output = output.resolve()
    if output == root.resolve() or root.resolve() in output.parents:
        raise ValueError("验证记录必须放在源码之外")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    record = {
        "schema_version": 2, "run_id": uuid.uuid4().hex, "scope": scope,
        "started_at": timestamp(), "source": None, "environment": None, "steps": [],
        "preparation": {"status": "running"}, "finalization": {"status": "not_started"},
        "passed": False, "scope_passed": False, "release_ready": False,
        "acceptance_status": "not_performed", "first_failure": None,
        "pending": ["目标环境验收"], "diagnostic_reruns": [], "cleanup_confirmed": False,
    }
    write(output / "VALIDATION.json", record)
    return record


def failure(record, message, code):
    if record["first_failure"] is None:
        record["first_failure"] = {"message": str(message), "exit_code": code, "at": timestamp()}
        record["exit_code"] = code
    record.update(passed=False, scope_passed=False, release_ready=False)


def phase(output, name, status, detail):
    log = output / f"{name}.log"
    log.write_text(json.dumps(detail, ensure_ascii=False, indent=2) + "\n")
    return {"status": status, "log_sha256": digest(log)}


def summary(output, record):
    result = "通过" if record.get("scope_passed", record.get("passed")) else "未通过"
    error = record.get("first_failure") or {}
    pending = record.get("pending", [])
    print(f"本范围结果：{record['scope']} {result}")
    unfinished = "无" if record.get("finished_at") else "运行尚未完成；清理状态请查看步骤回执"
    print(f"失败原因：{error.get('message', unfinished)}")
    print(f"待验证项：{', '.join(map(str, pending)) if pending else '未列出（完整目标验收未执行）'}")
    print(f"报告位置：{output / 'VALIDATION.json'}")


def verify_complete(output, record):
    if (record.get("schema_version") != 2 or record.get("passed") is not True
            or record.get("scope_passed") is not True or record.get("exit_code") != 0
            or record.get("cleanup_confirmed") is not True or "first_failure" not in record
            or record["first_failure"] is not None
            or not record.get("finished_at") or not record.get("run_id")):
        raise ValueError("需要完整且通过的第二版验证报告")
    environment = record["environment"]
    if record["scope"] != "preflight":
        owner = environment["cargo_owner"]
        if (owner["source_directory"] != environment["source_directory"]
                or not Path(environment["cargo_target"]).is_absolute()
                or type(owner["source_inode"]) is not int or type(owner["source_device"]) is not int):
            raise ValueError("编译缓存归属不完整")
    names = ("python3", "git") if record["scope"] == "preflight" else ("python3", "git", "cargo", "rustc", "node")
    for name in names:
        tool = environment["tools"][name]
        if not Path(tool["path"]).is_absolute() or not tool["version"]:
            raise ValueError("验证工具身份不完整")
    phases = {}
    for name in ("preparation", "finalization"):
        phase_record = record[name]
        path = output / f"{name}.log"
        if phase_record["status"] != "pass" or digest(path) != phase_record["log_sha256"]:
            raise ValueError("准备或清理阶段未通过，或日志摘要不匹配")
        phases[name] = json.loads(path.read_text())
    prepared = phases["preparation"]
    if (prepared["source"] != record["source"] or prepared["environment"] != environment
            or prepared["selected_checks"] != record["selected_checks"]
            or phases["finalization"]["source_unchanged"] is not True):
        raise ValueError("准备或收尾记录与候选身份不一致")
    steps = record["steps"]
    expected = [f"step-{number}" for number in range(1, len(steps) + 1)]
    receipts = phases["finalization"]["receipts"]
    if (not steps or [step["name"] for step in steps] != expected
            or [step["arguments"] for step in steps] != record["selected_checks"]
            or set(receipts) != set(expected)):
        raise ValueError("步骤、执行计划或清理集合不完整")
    for step in steps:
        verify_step(output, record, step, receipts)


def verify_step(output, record, step, receipts):
    name = step["name"]
    path = output / f"{name}.lease.json"
    lease = json.loads(path.read_text())
    if (step["returncode"] != 0 or step["cleanup_confirmed"] is not True
            or digest(output / f"{name}.log") != step["log_sha256"]
            or digest(output / f"{name}.stderr.log") != step["stderr_sha256"]
            or digest(path) != step["lease_sha256"] or receipts[name] != step["lease_sha256"]
            or lease["schema_version"] != 1 or lease["run_id"] != record["run_id"]
            or lease["name"] != name or lease["command"] != step["arguments"]
            or lease["cwd"] != record["environment"]["source_directory"]
            or lease["reason"] != "completed" or lease["cleanup_confirmed"] is not True
            or lease["command_returncode"] != 0 or not lease["group_started_at"]
            or any(type(lease[key]) is not int or lease[key] <= 0
                   for key in ("group_pid", "command_pid", "supervisor_pid", "owner_pid"))):
        raise ValueError("步骤日志或进程清理凭据与验证记录不一致")
