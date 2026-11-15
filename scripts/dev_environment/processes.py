import json
from contextlib import ExitStack
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from scripts.dev_environment.configuration import write_record


class Child:
    def __init__(self, root, name, command, environment, *, cwd=None, stderr_path=None):
        self.state = root / f"{name}-child.json"
        self.state.unlink(missing_ok=True)
        with ExitStack() as streams:
            log = streams.enter_context((root / f"{name}.log").open("ab"))
            errors = streams.enter_context(stderr_path.open("ab")) if stderr_path else log
            self.guard = subprocess.Popen([sys.executable, __file__, str(self.state), *command],
                                          cwd=cwd or root, env=environment, stdin=subprocess.DEVNULL,
                                          stdout=log, stderr=errors, start_new_session=True)

    def record(self):
        if self.guard.poll() is not None:
            raise RuntimeError("进程组守护者异常退出，不能再根据旧 PID 发信号")
        return json.loads(self.state.read_text()) if self.state.exists() else {"pid": None, "exit": None}

    def poll(self):
        return self.record()["exit"]

    @property
    def pid(self):
        return self.record()["pid"]


def guard(path, command):
    for number in (signal.SIGINT, signal.SIGTERM):
        signal.signal(number, lambda *_: None)
    process = subprocess.Popen(command)
    write_record(path, {"pid": process.pid, "exit": None})
    result = process.wait()
    write_record(path, {"pid": process.pid, "exit": result})
    while True:
        signal.pause()


def reap(children):
    errors = []
    for child in children:
        try:
            if child.guard.poll() is not None:
                raise RuntimeError("进程组守护者已退出，拒绝向旧 PID 发信号")
            os.killpg(child.guard.pid, signal.SIGINT)
        except (OSError, RuntimeError) as error:
            errors.append(str(error))
    deadline = time.monotonic() + 4
    try:
        while time.monotonic() < deadline and any(child.poll() is None for child in children):
            time.sleep(0.05)
    except (OSError, ValueError, RuntimeError) as error:
        errors.append(str(error))
    for child in children:
        try:
            if child.guard.poll() is not None:
                raise RuntimeError("进程组所有权丢失，未尝试杀旧 PID")
            os.killpg(child.guard.pid, signal.SIGKILL)
            child.guard.wait(timeout=4)
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            errors.append(str(error))
    groups = {str(child.guard.pid) for child in children}
    try:
        deadline = time.monotonic() + 2
        while True:
            rows = subprocess.check_output(["/bin/ps", "-axo", "pgid=,stat="],
                                           text=True, timeout=5).splitlines()
            active = [row for row in rows if len(row.split()) == 2
                      and row.split()[0] in groups and not row.split()[1].startswith("Z")]
            if not active:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError(f"本任务仍有未退出的进程组：{active}")
            time.sleep(0.05)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        errors.append(str(error))
    if errors:
        raise RuntimeError("；".join(errors))


if __name__ == "__main__":
    guard(Path(sys.argv[1]), sys.argv[2:])
