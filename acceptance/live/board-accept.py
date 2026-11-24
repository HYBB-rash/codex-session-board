#!/usr/bin/env python3
import argparse
import ast
from contextlib import nullcontext
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tarfile
import tempfile
from threading import Timer
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "acceptance"))
sys.path.insert(0, str(ROOT / "scripts"))

from candidate_receipt import candidate_identity, checked_archive
from collect_evidence import collect
from release_identity import json_file as read_json
from ssh_file_key import file_key_command


ACCEPTANCE = ROOT / "state/acceptance"
PACKAGES = ACCEPTANCE / "packages"
RUNS = ACCEPTANCE / "cli-runs"
HARNESS = ACCEPTANCE / "run-macos-public-surfaces.py"
PROBES = (
    HARNESS,
    ACCEPTANCE / "check-desktop-approval-0.3.25.mjs",
    ACCEPTANCE / "macos-board-acceptance-window.mjs",
)
VERSIONS = ("0.3.29", "0.3.30", "0.3.31")
REMOTE_EVENT = "BOARD_ACCEPT "
MAC_HELPER = Path(__file__).with_name("mac-copy-accept.py")
DOCTOR_PROBE = Path(__file__).with_name("doctor-window.mjs")


def candidate_package(version, manifest):
    default = ACCEPTANCE / "candidates" / version / "candidate.json"
    if manifest or default.is_file():
        identity, archives = candidate_identity(ACCEPTANCE, version, manifest)
        return archives["macos-arm64"], identity["archives"]["macos-arm64"]["archiveSha256"]
    return checked_archive(PACKAGES / f"session-board-{version}-macos-arm64.tar.gz")


def probe_profile():
    for statement in ast.parse(HARNESS.read_text()).body:
        if isinstance(statement, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "prior" for target in statement.targets
        ):
            call = statement.value
            if isinstance(call, ast.Call) and isinstance(call.func, ast.Name) and call.func.id == "Path" and len(call.args) == 1:
                return ast.literal_eval(call.args[0])
    raise ValueError("现有 Mac 验收脚本没有声明隔离测试副本")


def save(run, result):
    temporary = run / "status.tmp"
    temporary.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(run / "status.json")


def read_run(run_id):
    if not re.fullmatch(r"[a-f0-9]{16}", run_id):
        raise ValueError("运行 ID 格式错误")
    return read_json(RUNS / run_id / "status.json")


def ssh_command(host, control, destination):
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.@-]*", host):
        raise ValueError("--host 需要 SSH 配置别名，或 user@hostname")
    options = ["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=12",
               "-o", "StrictHostKeyChecking=yes", "-o", "ServerAliveInterval=10",
               "-o", "ServerAliveCountMax=3"]
    if control:
        options.extend(["-S", str(control)])
    return [*options, "--", host, shlex.join(destination)]


def remote_command(host, control, python, destination, temporary_key):
    if temporary_key:
        return file_key_command(host, control, python, destination)
    return nullcontext(ssh_command(host, control, destination))


def doctor(host, scenario, profile, target, fixture, remote_python, control, temporary_key):
    destination = [remote_python, "-B", "-c", MAC_HELPER.read_text(), "--doctor", profile, target, fixture]
    try:
        with remote_command(host, control, remote_python, destination, temporary_key) as command:
            result = subprocess.run(command, input=DOCTOR_PROBE.read_text(), capture_output=True, text=True, timeout=68)
    except subprocess.TimeoutExpired:
        print(json.dumps({"host": host, "scenario": scenario, "state": "host-probe-timeout",
                          "temporaryKey": temporary_key}))
        return 2
    observed = [json.loads(line[len(REMOTE_EVENT):]) for line in result.stdout.splitlines()
                if line.startswith(REMOTE_EVENT)]
    if observed:
        report = {"ssh": "connected", **observed[-1]}
    else:
        report = {
            "ssh": "unavailable" if result.returncode == 255 else "connected",
            "state": "ssh-unavailable" if result.returncode == 255 else "remote-command-unavailable",
            "exitCode": result.returncode, "reason": result.stderr.strip(),
        }
    print(json.dumps({"host": host, "scenario": scenario, "temporaryKey": temporary_key, **report},
                     ensure_ascii=False, indent=2))
    return 0 if result.returncode == 0 and report.get("state") == "ready" else 2


def run_remote(version, host, manifest=None, remote_python="python3", control=None, temporary_key=False, previous=None):
    package, digest = candidate_package(version, manifest)
    if previous and previous["archiveSha256"] != digest:
        raise ValueError("候选包已变化，请启动新验收")
    profile = probe_profile()
    destination = [remote_python, "-B", "-c", MAC_HELPER.read_text(), version, digest, profile]
    run_id = uuid4().hex[:16]
    RUNS.mkdir(mode=0o700, parents=True, exist_ok=True)
    run = RUNS / run_id
    run.mkdir(mode=0o700)
    result = {
        "runId": run_id, "candidate": version, "host": host, "mode": "copy",
        "scenario": "native-turns", "archiveSha256": digest,
        "startedAt": datetime.now(timezone.utc).isoformat(),
        "status": "running", "stage": "transport", "log": str(run / "run.log"),
        "manifest": str(manifest) if manifest else None, "remotePython": remote_python,
        "sshControl": str(control) if control else None, "temporaryKey": temporary_key,
    }
    if previous:
        result["resumedFrom"] = previous["runId"]
    save(run, result)
    print(json.dumps(result, ensure_ascii=False), flush=True)
    try:
        with tempfile.TemporaryFile() as payload:
            with tarfile.open(fileobj=payload, mode="w") as archive:
                for path in (package, *PROBES):
                    archive.add(path, arcname=path.name)
            payload.seek(0)
            with (run / "run.log").open("w") as log, remote_command(
                host, control, remote_python, destination, temporary_key
            ) as command:
                with subprocess.Popen(
                    command,
                    stdin=payload, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                ) as process:
                    deadline = Timer(960, process.kill)
                    deadline.start()
                    try:
                        for line in process.stdout:
                            log.write(line)
                            log.flush()
                            if line.startswith(REMOTE_EVENT):
                                event = json.loads(line[len(REMOTE_EVENT):])
                                result.update(event)
                                save(run, result)
                                print(json.dumps(event, ensure_ascii=False), flush=True)
                        exit_code = process.wait()
                    finally:
                        deadline.cancel()
                        if process.poll() is None:
                            process.kill()
        result["exitCode"] = exit_code
        if exit_code == 0 and result.get("cleanupVerified") and result.get("probePassed"):
            result["status"] = "passed"
        else:
            result["status"] = "failed"
    except (KeyboardInterrupt, OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        result.update(status="interrupted" if isinstance(error, KeyboardInterrupt) else "failed",
                      error=str(error))
    save(run, result)
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["status"] == "passed" else 1


def report(version):
    records = [read_json(path) for path in RUNS.glob("*/status.json")]
    gate = ACCEPTANCE / f"release-gate-{version}.json"
    print(json.dumps({
        "candidate": version,
        "runs": sorted((record for record in records if record["candidate"] == version),
                       key=lambda record: record["startedAt"]),
        "releaseGate": str(gate) if gate.is_file() else None,
    }, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description="诊断、运行及收集会话看板验收")
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run")
    run.add_argument("--candidate", required=True, choices=VERSIONS)
    run.add_argument("--mode", choices=["copy"], default="copy")
    run.add_argument("--scenario", choices=["native-turns"], default="native-turns")
    run.add_argument("--manifest", type=Path)
    check = commands.add_parser("doctor")
    check.add_argument("--scenario", choices=["native-turns", "native-move"], default="native-turns")
    check.add_argument("--profile", help="已有的隔离 Mac 测试副本目录")
    check.add_argument("--target-host-id", default="")
    check.add_argument("--thread-id", default="")
    for remote in (run, check):
        remote.add_argument("--host", required=True, help="现有 SSH 配置别名")
        remote.add_argument("--remote-python", default="python3")
        remote.add_argument("--ssh-control", type=Path, help="使用已经建立的 SSH ControlPath")
        remote.add_argument("--temporary-key", action="store_true", help="使用一条命令后即撤销的 SSH 文件密钥")
    gather = commands.add_parser("collect")
    gather.add_argument("--candidate", required=True)
    gather.add_argument("--manifest", type=Path)
    gather.add_argument("--state-dir", type=Path, default=ACCEPTANCE)
    gather.add_argument("--import-evidence", action="append", type=Path, default=[])
    gather.add_argument("--gate", action="store_true")
    for name in ("status", "resume"):
        commands.add_parser(name).add_argument("run_id")
    commands.add_parser("report").add_argument("--candidate", required=True, choices=VERSIONS)
    args = parser.parse_args()
    if args.command == "doctor":
        if args.scenario == "native-move" and not (args.target_host_id and args.thread_id):
            parser.error("native-move 需要 --target-host-id 和 --thread-id")
        if args.thread_id and not args.target_host_id:
            parser.error("--thread-id 需要 --target-host-id")
    try:
        if args.command == "run":
            manifest = args.manifest.absolute() if args.manifest else None
            control = args.ssh_control.absolute() if args.ssh_control else None
            return run_remote(args.candidate, args.host, manifest, args.remote_python, control, args.temporary_key)
        if args.command == "doctor":
            return doctor(args.host, args.scenario, args.profile or probe_profile(), args.target_host_id,
                          args.thread_id, args.remote_python, args.ssh_control, args.temporary_key)
        if args.command == "collect":
            return collect(ROOT, args.state_dir.resolve(), args.candidate, args.manifest,
                           args.import_evidence, args.gate)
        if args.command == "report":
            report(args.candidate)
        else:
            record = read_run(args.run_id)
            if args.command == "status":
                print(json.dumps(record, ensure_ascii=False, indent=2))
            elif record["status"] in {"failed", "interrupted"}:
                manifest = Path(record["manifest"]) if record.get("manifest") else None
                return run_remote(record["candidate"], record["host"], manifest,
                                  record.get("remotePython", "python3"), record.get("sshControl"),
                                  record.get("temporaryKey", False), record)
            else:
                raise ValueError("只允许重跑失败或中断的场景")
    except (OSError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
