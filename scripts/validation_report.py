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
        "pending": [], "diagnostic_reruns": [], "cleanup_confirmed": False,
    }
    write(output / "VALIDATION.json", record)
    return record


def failure(record, message, code):
    if record["first_failure"] is None:
        record["first_failure"] = {"message": str(message), "exit_code": code, "at": timestamp()}
    record.update(passed=False, scope_passed=False, exit_code=code)


def phase(output, name, status, detail):
    log = output / f"{name}.log"
    log.write_text(json.dumps(detail, ensure_ascii=False, indent=2) + "\n")
    return {"status": status, "log_sha256": digest(log)}


def summary(output, record):
    result = "通过" if record.get("scope_passed", record.get("passed")) else "未通过"
    error = record.get("first_failure") or {}
    pending = record.get("pending", [])
    print(f"本范围结果：{record['scope']} {result}")
    print(f"失败原因：{error.get('message', '无')}")
    print(f"待验证项：{', '.join(map(str, pending)) if pending else '无（完整目标验收未执行）'}")
    print(f"报告位置：{output / 'VALIDATION.json'}")
