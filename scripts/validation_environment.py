import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile


def choose_python(root, environment):
    explicit = environment.get("CSB_VALIDATION_PYTHON")
    data = Path(environment.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    candidates = [explicit] if explicit else [
        shutil.which("python3"), sys.executable,
        *(shutil.which(f"python3.{minor}") for minor in range(15, 10, -1)),
        str(root / ".venv/bin/python"),
        str(data / "codex-session-board/delivery-venv/bin/python"),
    ]
    rejected = []
    for candidate in dict.fromkeys(item for item in candidates if item):
        path = shutil.which(candidate, path=environment.get("PATH"))
        if path is None:
            rejected.append(f"{candidate}: 不存在或不可执行")
            continue
        try:
            result = subprocess.run(
                [path, "-c", "import json,sys,tomllib; "
                 "assert sys.version_info >= (3,11); print(json.dumps(sys.version))"],
                capture_output=True, text=True, timeout=10, env=environment)
            if result.returncode == 0:
                return {"path": os.path.abspath(path), "version": json.loads(result.stdout)}
            rejected.append(f"{path}: 需要 Python 3.11+ 和 tomllib")
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            rejected.append(f"{path}: {error}")
    raise RuntimeError("设置 CSB_VALIDATION_PYTHON 为已有 Python 3.11+ 可执行文件；" + "；".join(rejected))


def cache_identity(root):
    return {"schema_version": 1, "source_directory": str(root.resolve())}


def claim_cache(root, target):
    target = target.resolve()
    owner = target / ".validation-owner.json"
    expected = cache_identity(root)
    target.mkdir(parents=True, exist_ok=True)
    if not owner.exists():
        if any(target.iterdir()):
            raise RuntimeError(f"缓存没有归属凭据，请指定新的空 CARGO_TARGET_DIR：{target}")
        try:
            with owner.open("x") as stream:
                json.dump(expected, stream)
        except FileExistsError:
            pass
    if json.loads(owner.read_text()) != expected:
        raise RuntimeError(f"缓存属于其他源码目录，请使用独立 CARGO_TARGET_DIR：{target}")
    return str(target)


def tool(command, environment):
    path = shutil.which(command, path=environment.get("PATH"))
    if path is None:
        raise RuntimeError(f"缺少 {command}；请提供已安装工具的 PATH")
    result = subprocess.run([path, "--version"], text=True, capture_output=True,
                            timeout=10, env=environment, check=True)
    return {"path": os.path.abspath(path), "version": result.stdout.strip()}


def resolve(root, output, scope, python=None, staged=False):
    environment = dict(os.environ)
    tools = {"python3": python or choose_python(root, environment)}
    names = ("git",) if scope == "preflight" else ("git", "cargo", "rustc", "node")
    for name in names:
        tools[name] = tool(name, environment)
    temporary = Path("/private/tmp" if sys.platform == "darwin" else
                     environment.get("TMPDIR", tempfile.gettempdir())).resolve()
    if not temporary.is_dir():
        raise RuntimeError(f"临时目录不存在：{temporary}")
    target = None
    if scope != "preflight":
        requested = root / "target" if staged else Path(
            environment.get("CARGO_TARGET_DIR", str(root / "target/validation")))
        target = claim_cache(root, requested if requested.is_absolute() else root / requested)
        environment["CARGO_TARGET_DIR"] = target
    binary = output / "tools"
    binary.mkdir(mode=0o700)
    for name, description in tools.items():
        wrapper = binary / name
        wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(description["path"]) + ' "$@"\n')
        wrapper.chmod(0o700)
    environment.update(PATH=str(binary) + os.pathsep + environment.get("PATH", ""),
                       TMPDIR=str(temporary), CSB_VALIDATION_PYTHON=tools["python3"]["path"])
    metadata = {"tools": tools, "platform": sys.platform, "machine": platform.machine(),
                "source_directory": str(root.resolve()),
                "temporary_directory": str(temporary), "cargo_target": target}
    return environment, metadata
