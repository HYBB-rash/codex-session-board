import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.dev_environment.processes import Child, reap
from scripts.validation_report import digest, timestamp, write


def supervise(request):
    output = Path(request["output"])
    name = request["name"]
    receipt = {
        "schema_version": 1, "run_id": request["run_id"], "name": name,
        "owner_pid": os.getppid(), "supervisor_pid": os.getpid(),
        "command": request["command"], "cwd": request["cwd"],
        "started_at": timestamp(), "group_pid": None, "command_pid": None,
        "command_returncode": None, "reason": "running", "cleanup_confirmed": False,
    }
    path = output / f"{name}.lease.json"
    write(path, receipt)
    child = None
    try:
        child = Child(output, name, request["command"], dict(os.environ), cwd=request["cwd"],
                      stderr_path=output / f"{name}.stderr.log")
        receipt["group_pid"] = child.guard.pid
        receipt["group_started_at"] = subprocess.check_output(
            ["/bin/ps", "-p", str(child.guard.pid), "-o", "lstart="],
            text=True, timeout=5).strip()
        write(path, receipt)
        deadline = time.monotonic() + request["timeout"]
        while True:
            state = child.record()
            receipt["command_pid"] = state["pid"]
            if state["exit"] is not None:
                receipt.update(command_returncode=state["exit"], reason="completed")
                break
            if select.select([sys.stdin], [], [], 0.05)[0] and not os.read(0, 1):
                receipt["reason"] = "owner_lost"
                break
            if time.monotonic() >= deadline:
                receipt["reason"] = "timeout"
                break
    except Exception as error:
        receipt.update(reason="infrastructure_error", error=str(error))
    finally:
        try:
            if child is not None:
                reap([child])
            receipt["cleanup_confirmed"] = True
        except Exception as error:
            receipt.update(reason="cleanup_failed", error=str(error))
        receipt["finished_at"] = timestamp()
        write(path, receipt)
    return 0 if receipt["cleanup_confirmed"] else 3


def execute(name, arguments, output, *, root, environment, timeout, run_id):
    started = time.monotonic()
    request = {"output": str(output), "name": name, "command": arguments, "cwd": str(root),
               "timeout": timeout, "run_id": run_id}
    write(output / f"{name}.request.json", request)
    with (output / f"{name}.supervisor.log").open("w") as log:
        supervisor = subprocess.Popen(
            [sys.executable, __file__, str(output / f"{name}.request.json")],
            stdin=subprocess.PIPE, stdout=log, stderr=log, env=environment,
            start_new_session=True)
        try:
            supervisor.wait()
        finally:
            supervisor.stdin.close()
            supervisor.wait(timeout=20)
    path = output / f"{name}.lease.json"
    receipt = json.loads(path.read_text())
    if supervisor.returncode != 0 or receipt["cleanup_confirmed"] is not True:
        raise RuntimeError(f"{name} 未确认清理完成：{receipt}")
    if receipt["reason"] != "completed":
        raise RuntimeError(f"{name} {receipt['reason']}：{receipt.get('error', '')}")
    return {
        "name": name, "arguments": arguments, "returncode": receipt["command_returncode"],
        "seconds": time.monotonic() - started, "log_sha256": digest(output / f"{name}.log"),
        "stderr_sha256": digest(output / f"{name}.stderr.log"),
        "lease_sha256": digest(path), "cleanup_confirmed": True,
    }


def cleanup_receipts(output, run_id):
    receipts = {}
    for request in output.glob("step-*.request.json"):
        name = request.name.removesuffix(".request.json")
        path = output / f"{name}.lease.json"
        receipt = json.loads(path.read_text())
        if (receipt["run_id"] != run_id or receipt["name"] != name
                or receipt["cleanup_confirmed"] is not True):
            raise RuntimeError(f"{name} 的进程清理凭据不完整")
        receipts[name] = digest(path)
    return receipts


if __name__ == "__main__":
    sys.exit(supervise(json.loads(Path(sys.argv[1]).read_text())))
