import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def program(name, environment):
    path = shutil.which(name, path=environment.get("PATH"))
    if path is None:
        raise ValueError(f"Missing checker build tool: {name}")
    path = Path(path).resolve()
    for _ in range(4):
        prefix = path.read_bytes()[:8192]
        if not prefix.startswith(b"#!/usr/bin/env bash\nexec "):
            break
        arguments = shlex.split(prefix.decode().splitlines()[1].removeprefix("exec "))
        if len(arguments) != 2 or arguments[1] != "$@" or not Path(arguments[0]).is_absolute():
            break
        path = Path(arguments[0]).resolve(strict=True)
    return {"sha256": digest(path)}


def path_identity(environment):
    entries = []
    for value in environment.get("PATH", "").split(os.pathsep):
        directory = Path(value or ".").resolve()
        if not directory.is_dir():
            continue
        files = sorted(directory.iterdir()) if directory.name == "tools" and directory.is_dir() else []
        wrappers = {}
        for path in files:
            if not path.is_file():
                break
            text = path.read_bytes()
            if not (text.startswith(b"#!/usr/bin/env bash\nexec ") and text.endswith(b' "$@"\n')):
                break
            wrappers[path.name] = hashlib.sha256(text).hexdigest()
        else:
            if wrappers:
                continue
        entries.append(str(directory))
    return entries


def source_inputs(root):
    paths = {*root.joinpath("scripts").rglob("*.rs"),
             root / "scripts/check", root / "scripts/checker_cache.py",
             root / "scripts/checker/Cargo.toml", root / "scripts/checker/Cargo.lock"}
    paths = {path for path in paths if "target" not in path.relative_to(root).parts}
    if any(path.is_symlink() or not path.is_file() for path in paths):
        raise ValueError("Checker source inputs must be regular files")
    return {path.relative_to(root).as_posix(): digest(path) for path in sorted(paths)}


def configuration(root, environment):
    cargo_home = Path(environment.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    paths = {cargo_home / name for name in ("config", "config.toml")}
    for directory in (root, *root.parents):
        paths.update(directory / ".cargo" / name for name in ("config", "config.toml"))
        paths.update(directory / name for name in ("rust-toolchain", "rust-toolchain.toml"))
    return {str(path): digest(path) for path in sorted(paths) if path.is_file()}


def describe(root, environment):
    versions = {}
    for name, arguments in (("cargo", ["--version"]), ("rustc", ["-vV"])):
        versions[name] = {
            **program(name, environment),
            "version": subprocess.check_output([name, *arguments], cwd=root, env=environment,
                                              text=True, timeout=30).strip(),
        }
    host = next(line.removeprefix("host: ") for line in versions["rustc"]["version"].splitlines()
                if line.startswith("host: "))
    ignored = {"CARGO_TARGET_DIR", "CARGO_MAKEFLAGS"}
    selected = {name: value for name, value in environment.items()
                if name not in ignored
                and not name.startswith(("CARGO_PKG_", "CARGO_MANIFEST_", "CARGO_BIN_EXE_", "CARGO_CRATE_"))
                and (name.startswith(("CARGO_", "RUST", "CC_", "CXX_", "AR_", "PKG_CONFIG"))
                     or name in {"CC", "CXX", "AR", "RANLIB", "CFLAGS", "CXXFLAGS", "CPPFLAGS",
                                 "LDFLAGS", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "LIBRARY_PATH",
                                 "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "CPATH"})}
    commands = {"cc", "clang", "gcc", "ar", "ld"}
    commands.update(value for name, value in selected.items()
                    if name in {"RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CC", "CXX", "AR"}
                    or name.endswith("_LINKER"))
    programs = {name: program(name, environment) for name in sorted(commands)
                if name and shutil.which(name, path=environment.get("PATH"))}
    inputs = source_inputs(root)
    build = {"schema_version": 1, "tools": versions, "host": host,
             "platform": sys.platform, "machine": platform.machine(), "programs": programs,
             "configuration": configuration(root, environment),
             "environment_sha256": hashlib.sha256(encoded(selected)).hexdigest(),
             "path": path_identity(environment), "profile": "dev", "features": [],
             "manifest_sha256": inputs["scripts/checker/Cargo.toml"],
             "lock_sha256": inputs["scripts/checker/Cargo.lock"]}
    return {"build": build, "inputs": inputs}


def existing(directory, identity):
    if not directory.exists():
        return None
    if directory.is_symlink():
        raise ValueError("Checker cache entry cannot be a symlink")
    manifest = json.loads((directory / "CHECKER.json").read_text())
    binary = directory / "project-check"
    if (manifest.get("identity") != identity or binary.is_symlink()
            or not binary.is_file() or not os.access(binary, os.X_OK)
            or manifest.get("binary_sha256") != digest(binary)):
        raise ValueError("Checker cache identity or executable digest differs")
    return binary


def build(root, environment, identity, cache, destination):
    build_key = hashlib.sha256(encoded(identity["build"])).hexdigest()
    target = cache / "build" / build_key
    target.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="checker-", dir=cache / "staging") as temporary:
        staging = Path(temporary)
        source = staging / "source"
        for name in identity["inputs"]:
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / name, path)
        command = ["cargo", "build", "--locked", "--offline", "--manifest-path",
                   str(source / "scripts/checker/Cargo.toml"), "--bin", "project-check",
                   "--target", identity["build"]["host"], "--target-dir", str(target),
                   "--message-format=json"]
        process = subprocess.run(command, cwd=root, env=environment, text=True,
                                 capture_output=True, timeout=600)
        if process.returncode:
            raise RuntimeError(process.stderr[-12000:])
        artifacts = [json.loads(line) for line in process.stdout.splitlines() if line.strip()]
        binaries = {Path(item["executable"]).resolve() for item in artifacts
                    if item.get("target", {}).get("name") == "project-check" and item.get("executable")}
        if len(binaries) != 1:
            raise ValueError("Cargo did not identify the actual checker executable")
        binary = binaries.pop()
        if not binary.is_relative_to(target.resolve()):
            raise ValueError("Cargo checker executable belongs to a different build directory")
        if describe(root, environment) != identity:
            raise ValueError("Checker source or build conditions changed during compilation")
        entry = staging / "entry"
        entry.mkdir()
        shutil.copy2(binary, entry / "project-check")
        (entry / "project-check").chmod(0o755)
        (entry / "CHECKER.json").write_text(json.dumps(
            {"identity": identity, "binary_sha256": digest(binary)}, sort_keys=True) + "\n")
        entry.rename(destination)


def resolve(root, environment=None):
    root = Path(root).resolve(strict=True)
    environment = dict(os.environ if environment is None else environment)
    environment["PATH"] = os.pathsep.join(
        str(Path(value) if Path(value).is_absolute() else root / value)
        for value in environment.get("PATH", "").split(os.pathsep))
    started = time.monotonic()
    identity = describe(root, environment)
    key = hashlib.sha256(encoded(identity)).hexdigest()
    base = Path(environment.get("XDG_CACHE_HOME", str(Path.home() / ".cache")))
    cache = Path(environment.get("CSB_CHECKER_CACHE_DIR", str(base / "codex-session-board/checker"))).resolve()
    destination = cache / "entries" / key
    for directory in ("entries", "locks", "staging"):
        (cache / directory).mkdir(parents=True, exist_ok=True)
    binary = existing(destination, identity)
    hit = binary is not None
    if binary is None:
        lock_key = hashlib.sha256(encoded(identity["build"])).hexdigest()
        with (cache / "locks" / f"{lock_key}.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            binary = existing(destination, identity)
            hit = binary is not None
            if binary is None:
                build(root, environment, identity, cache, destination)
                binary = existing(destination, identity)
    if describe(root, environment) != identity:
        raise ValueError("Checker source or build conditions changed before execution")
    return binary, {"scope": "checker_cache", "key": key, "cache_hit": hit,
                    "seconds": time.monotonic() - started, "binary_sha256": digest(binary),
                    "binary": str(binary), "source": str(root)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    try:
        binary, report = resolve(arguments.project)
        if os.environ.get("CSB_CHECKER_CACHE_REPORT"):
            path = Path(os.environ["CSB_CHECKER_CACHE_REPORT"]).resolve()
            if path.is_relative_to(arguments.project.resolve()):
                raise ValueError("Checker cache reports must remain outside the checked source")
            path.write_text(json.dumps(report, indent=2) + "\n")
        print("[工具缓存] " + ("命中 " if report["cache_hit"] else "构建 ")
              + report["key"][:12] + f" {report['seconds']:.3f}s", file=sys.stderr)
        forwarded = arguments.arguments[1:] if arguments.arguments[:1] == ["--"] else arguments.arguments
        return subprocess.call([str(binary), *forwarded], cwd=arguments.project)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"[检查错误] 固定检查器无法运行：{error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
