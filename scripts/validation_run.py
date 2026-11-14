import json
import sys

from scripts import validation_environment as environment
from scripts import validation_process as processes
from scripts import validation_report as report


class CheckFailed(Exception):
    pass


def begin(arguments, root, scope):
    output = arguments.output.resolve()
    if not arguments.initialized_run:
        return report.create(output, root, scope)
    record = json.loads((output / "VALIDATION.json").read_text())
    if (record["run_id"] != arguments.initialized_run or record["schema_version"] != 2
            or record["scope"] != scope or record["steps"] or record["passed"]
            or record["source"] is not None):
        raise ValueError("初始化记录与本次请求不一致")
    return record


def prepare(root, output, record, source, commands, *, staged=False):
    record["source"] = source
    child, metadata = environment.resolve(root, output, record["scope"],
                                          record.get("bootstrap_python"), staged=staged)
    record.update(environment=metadata, selected_checks=commands)
    record["preparation"] = report.phase(output, "preparation", "pass", {
        "source": source, "environment": metadata, "selected_checks": commands})
    report.write(output / "VALIDATION.json", record)
    return child


def steps(root, output, record, commands, child, timeout):
    for number, command in enumerate(commands, len(record["steps"]) + 1):
        name = f"step-{number}"
        print(f"[验证] {name}: {' '.join(command)}", flush=True)
        result = processes.execute(name, command, output, root=root, environment=child,
                                   timeout=timeout, run_id=record["run_id"])
        record["steps"].append(result)
        report.write(output / "VALIDATION.json", record)
        if result["returncode"] != 0:
            for suffix, stream in ((".log", sys.stdout), (".stderr.log", sys.stderr)):
                print((output / (name + suffix)).read_text(errors="replace")[-12000:],
                      file=stream, flush=True)
            raise CheckFailed(f"{name} 退出 {result['returncode']}；详情见 {output / (name + '.log')}")


def reject(output, record, error, code):
    report.failure(record, error, code)
    if record["preparation"]["status"] == "running":
        record["preparation"] = report.phase(output, "preparation", "fail", {"error": str(error)})
    report.write(output / "VALIDATION.json", record)


def finish(output, record, unchanged):
    detail = {"source_unchanged": unchanged, "receipts": {}}
    try:
        detail["receipts"] = processes.cleanup_receipts(output, record["run_id"])
        record["cleanup_confirmed"] = True
    except Exception as error:
        detail["cleanup_error"] = str(error)
        reject(output, record, error, 3)
    if unchanged is False:
        reject(output, record, "验证期间源码、暂存内容或提交身份发生变化", 1)
    complete = record["cleanup_confirmed"] and unchanged is True
    record["finalization"] = report.phase(output, "finalization", "pass" if complete else "fail", detail)
    passed = complete and record["preparation"]["status"] == "pass" and record["first_failure"] is None
    record.update(passed=passed, scope_passed=passed, finished_at=report.timestamp(),
                  exit_code=record.get("exit_code", 0 if passed else 3))
    report.write(output / "VALIDATION.json", record)
    report.summary(output, record)
    return record["exit_code"]
