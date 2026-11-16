import argparse
import json
import math
import os
from pathlib import Path
import sys
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import validation_environment, validation_report as report


def parse():
    parser = argparse.ArgumentParser()
    parser.add_argument("--scope", choices=("pr", "candidate", "preflight"), default="candidate")
    parser.add_argument("--stage", choices=("ci", "submit"))
    parser.add_argument("--base", default="origin/main")
    parser.add_argument("--integration-base")
    parser.add_argument("--project", type=Path)
    parser.add_argument("--expect-head")
    parser.add_argument("--risk", action="append", choices=("connection", "package", "lifecycle"), default=[])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout", type=float, default=1800)
    parser.add_argument("--staged", action="store_true")
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--initialized-run", help=argparse.SUPPRESS)
    arguments = parser.parse_args()
    if not math.isfinite(arguments.timeout) or arguments.timeout <= 0:
        parser.error("--timeout 必须大于 0")
    if not (arguments.output or arguments.staged or arguments.plan or arguments.report):
        parser.error("运行验证需要 --output，或使用 --plan / --report")
    if (arguments.project or arguments.expect_head or arguments.risk or arguments.stage) and (
            arguments.scope != "pr" or arguments.staged):
        parser.error("PR source and risk options are only valid for --scope pr")
    if arguments.scope == "pr" and (
            arguments.staged or arguments.integration_base or arguments.initialized_run
            or arguments.timeout != 1800):
        parser.error("PR checks use a fixed 1800 second total budget and their own initialization")
    return arguments


def interrupted(output, record, error):
    report.failure(record, error, 3)
    record["preparation"] = report.phase(output, "preparation", "fail", {"error": str(error)})
    record["finalization"] = report.phase(output, "finalization", "pass", {"receipts": {}})
    record.update(cleanup_confirmed=True, finished_at=report.timestamp())
    report.write(output / "VALIDATION.json", record)
    report.summary(output, record)
    return 3


def main():
    arguments = parse()
    if arguments.report:
        output = arguments.report.resolve()
        record = json.loads((output / "VALIDATION.json").read_text())
        if record.get("schema_version") == 1:
            print("历史报告：仅供读取，不能用于新候选晋升")
        elif record.get("schema_version") != 2:
            raise ValueError("无法读取未知版本的验证报告")
        elif record.get("passed") is True:
            report.verify_complete(output, record)
        report.summary(output, record)
        return 0
    output = arguments.output or Path(tempfile.gettempdir()) / ("board-validation-" + uuid.uuid4().hex)
    output = output.resolve()
    direct_pr = arguments.scope == "pr" and not arguments.staged
    record = None if arguments.plan or direct_pr else report.create(
        output, ROOT, "staged" if arguments.staged else arguments.scope)
    try:
        python = validation_environment.choose_python(ROOT, dict(os.environ))
        if record is not None:
            record["bootstrap_python"] = python
            report.write(output / "VALIDATION.json", record)
        script = "validate_staged.py" if arguments.staged else "validate_candidate.py"
        command = [python["path"], str(ROOT / "scripts" / script), *sys.argv[1:]]
        if record is not None:
            command.extend(["--initialized-run", record["run_id"]])
        if arguments.output is None:
            command.extend(["--output", str(output)])
        child = dict(os.environ, CSB_VALIDATION_PYTHON=python["path"])
        if direct_pr and sys.platform == "darwin":
            child["TMPDIR"] = "/private/tmp"
        os.execve(python["path"], command, child)
    except Exception as error:
        if record is None:
            raise
        return interrupted(output, record, error)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError) as error:
        print(f"验证初始化失败：{error}", file=sys.stderr)
        sys.exit(3)
