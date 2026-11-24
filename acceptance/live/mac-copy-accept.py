import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import signal
import subprocess
import sys
import tempfile
import time


TMP = Path("/private/tmp")
APP = "state/Codex Enhancer.app"
PROFILE_DIRECTORIES = ("user-data", "codex-home")
NODE = "/opt/homebrew/bin/node"


def event(stage, **fields):
    print("BOARD_ACCEPT " + json.dumps({"stage": stage, **fields}, ensure_ascii=False), flush=True)


def require(condition, error):
    if not condition:
        raise RuntimeError(error)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def app_processes(package):
    scope = f"{package / APP}/"
    processes = subprocess.check_output(["/bin/ps", "-ax", "-o", "pid=,comm="], text=True, timeout=10)
    result = {}
    for line in processes.splitlines():
        pid, executable = line.strip().split(maxsplit=1)
        if executable.startswith((scope, scope.removeprefix("/private"))):
            result[int(pid)] = executable
    return result


def stop_copy(package):
    for pid in app_processes(package):
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            continue
    for _ in range(80):
        if not app_processes(package):
            return
        time.sleep(0.25)
    raise RuntimeError("测试副本退出超时")


def cli(package, action):
    return subprocess.check_output(
        [NODE, str(package / "dist/bin/codex-enhancer.js"), action],
        cwd=package, env={**os.environ, "CODEX_ENHANCER_SOURCE": "/Applications/ChatGPT.app"},
        text=True, timeout=100,
    )


def uninstalled(package):
    return cli(package, "status").splitlines()[0] in {"result: not-installed", "result: removed"}


def run_probe(stage):
    with subprocess.Popen(
        [sys.executable, "-u", "-B", str(stage / "run-macos-public-surfaces.py"), str(stage), "--workflow"],
        start_new_session=True,
    ) as probe:
        try:
            return probe.wait(timeout=650)
        except BaseException:
            os.killpg(probe.pid, signal.SIGTERM)
            try:
                probe.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(probe.pid, signal.SIGKILL)
            raise


def cleanup(package, prior, prior_running, config, original_config):
    stop_copy(package)
    for name in PROFILE_DIRECTORIES:
        path = package / "state" / name
        backup = path.with_name(f"{name}.fresh")
        if path.is_symlink():
            require(path.readlink() == prior / "state" / name, f"隔离目录链接目标异常：{name}")
            path.unlink()
        if backup.exists() or backup.is_symlink():
            require(backup.is_dir() and not backup.is_symlink() and not path.exists(),
                    f"隔离目录无法自动复位：{name}")
            backup.rename(path)
    if not uninstalled(package):
        cli(package, "uninstall")
    require(uninstalled(package), "无法确认候选副本已卸载")
    if config.read_bytes() != original_config:
        stop_copy(prior)
        config.write_bytes(original_config)
        require(config.read_bytes() == original_config, "测试配置恢复失败")
    if prior_running and not app_processes(prior):
        require(cli(prior, "launch").startswith("pid: "), "旧测试副本未恢复运行")
    if not prior_running and app_processes(prior):
        stop_copy(prior)


def test_profile(profile):
    prior = Path(profile)
    require(prior.resolve(strict=True) == prior and prior.is_relative_to(TMP)
            and prior.stat().st_uid == os.getuid() and "macos-arm64" in prior.name, "旧测试副本路径无效")
    app = prior / APP
    require(app.is_dir() and not app.is_symlink() and app.resolve() == app, "旧测试副本应用路径无效")
    for name in PROFILE_DIRECTORIES:
        path = prior / "state" / name
        require(path.is_dir() and not path.is_symlink() and path.stat().st_uid == os.getuid(),
                f"旧测试副本缺少专用目录：{name}")
    config = prior / "state/codex-home/config.toml"
    require(config.is_file() and not config.is_symlink(), "旧测试副本缺少独立配置")
    return prior, config


def desktop_ready():
    return subprocess.run(
        ["/bin/launchctl", "print", f"gui/{os.getuid()}"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5,
    ).returncode == 0


def codex_bundle(app=Path("/Applications/ChatGPT.app")):
    with (app / "Contents/Info.plist").open("rb") as bundle:
        return plistlib.load(bundle)


def native_state(probe, target, fixture):
    if "error" in probe:
        return "native-window-unavailable"
    if probe.get("account", {}).get("type") != "chatgpt":
        return "native-login-unavailable"
    if target and not probe.get("connection", {}).get("connected"):
        return "native-connection-unavailable"
    if fixture and not probe.get("card", {}).get("readable"):
        return "fixture-unavailable"
    return "ready"


def doctor(profile, target, fixture):
    result = {"os": platform.system().lower(), "arch": platform.machine()}
    if (result["os"], result["arch"]) != ("darwin", "arm64") or os.getuid() == 0:
        event("doctor", state="wrong-host", **result)
        return 2
    result["graphicalLogin"] = desktop_ready()
    if not result["graphicalLogin"]:
        event("doctor", state="graphical-session-unavailable", **result)
        return 2
    try:
        prior, _ = test_profile(profile)
        installed = codex_bundle(prior / APP)
        result["codexBuild"] = installed["CFBundleShortVersionString"]
        result["profileValid"] = True
    except (OSError, RuntimeError, KeyError, ValueError) as error:
        event("doctor", state="test-profile-unavailable", reason=str(error), **result)
        return 2
    processes = app_processes(prior)
    if not processes:
        event("doctor", state="test-window-not-running", **result)
        return 2
    result["testWindowRunning"] = True
    try:
        debugger = prior / "state/user-data/DevToolsActivePort"
        require(debugger.is_file() and not debugger.is_symlink(), "测试副本没有有效的调试端口回执")
        port = int(debugger.read_text().splitlines()[0])
        require(0 < port <= 65535, "调试端口不合法")
        listeners = subprocess.run(
            ["/usr/sbin/lsof", "-nP", "-a", "-p", ",".join(map(str, processes)),
             f"-iTCP:{port}", "-sTCP:LISTEN", "-t"],
            capture_output=True, text=True, timeout=7,
        )
        require(listeners.returncode == 0 and bool(listeners.stdout.strip()), "调试端口未归属当前测试副本")
    except (OSError, ValueError, IndexError, RuntimeError, subprocess.SubprocessError) as error:
        event("doctor", state="debug-endpoint-unavailable", reason=str(error), **result)
        return 2
    result["debugEndpointOwned"] = True
    probe = subprocess.run(
        [NODE, "--input-type=module", "-", str(port), target, fixture],
        input=sys.stdin.read(), capture_output=True, text=True, timeout=40,
    )
    if not probe.stdout:
        event("doctor", state="native-probe-unavailable", **result)
        return 2
    native = json.loads(probe.stdout)
    result["native"] = native
    state = "native-window-unavailable" if probe.returncode else native_state(native, target, fixture)
    event("doctor", state=state, **result)
    return 0 if state == "ready" else 2


def main():
    if sys.argv[1:2] == ["--doctor"]:
        return doctor(*sys.argv[2:])
    version, expected, profile = sys.argv[1:]
    event("preflight")
    require((platform.system(), platform.machine()) == ("Darwin", "arm64"), "验收宿主必须是 macOS arm64")
    require(os.getuid() != 0, "请使用已有的隔离测试账号")
    require(desktop_ready(), "测试账号需要已有的图形登录")
    prior, config = test_profile(profile)
    bundle = codex_bundle()

    lock_path = TMP / f"board-accept-copy-{os.getuid()}.lock"
    with os.fdopen(os.open(lock_path, os.O_RDWR | os.O_NOFOLLOW | os.O_CREAT, 0o600), "r+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        stage = Path(tempfile.mkdtemp(prefix=f"enhancer-final-0{version.rsplit('.', 1)[-1]}.", dir=TMP))
        event("stage-package", remoteStage=str(stage), codexBuild=bundle["CFBundleVersion"],
              hostInfo={"os": "darwin", "arch": "arm64", "machine": platform.node(),
                        "codexBuild": bundle["CFBundleShortVersionString"]})
        subprocess.run(["/usr/bin/tar", "-xf", "-", "-C", stage], check=True, timeout=60)
        name = f"session-board-{version}-macos-arm64"
        archive = stage / f"{name}.tar.gz"
        require(digest(archive) == expected, "Mac 宿主归档 SHA-256 不匹配")
        subprocess.run(["/usr/bin/tar", "-xzf", archive, "-C", stage], check=True, timeout=60)
        package = stage / name
        release = json.loads((package / "release.json").read_text())
        require(release["version"] == version and uninstalled(package), "候选安装状态或版本异常")
        for name in PROFILE_DIRECTORIES:
            path = package / "state" / name
            require(not path.is_symlink() and (not path.exists() or path.is_dir())
                    and not os.path.lexists(path.with_name(f"{name}.fresh")), f"候选隔离目录状态异常：{name}")
        original_config = config.read_bytes()
        prior_running = bool(app_processes(prior))
        event("native-turns", archiveVerified=True)
        try:
            code = run_probe(stage)
        finally:
            event("cleanup")
            cleanup(package, prior, prior_running, config, original_config)
            event("cleanup", cleanupVerified=True)
        receipts = sorted((package / "state/acceptance").glob(f"desktop-approval-{version}-*.jsonl"))
        required = {
            "trusted_approval_and_confirmed_resume", "native_retry_preserved_running_and_note",
            "native_clarification_reviewed", "fixture_archived", "config_restored",
        }
        events = {
            row["event"]: row for receipt in receipts for line in receipt.read_text().splitlines()
            if (row := json.loads(line)).get("version") == version
        }
        restored = events.get("config_restored", {})
        passed = code == 0 and required <= events.keys() and restored.get("exactBytes") and restored.get("codexExactBytes")
        event("finished", probePassed=bool(passed), probeExitCode=code, receipts=[str(path) for path in receipts])
        return 0 if passed else 1


def interrupt(_signum, _frame):
    raise KeyboardInterrupt


if __name__ == "__main__":
    for sig in (signal.SIGHUP, signal.SIGTERM):
        signal.signal(sig, interrupt)
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError, KeyboardInterrupt) as error:
        event("error", reason=str(error) or type(error).__name__)
        sys.exit(1)
