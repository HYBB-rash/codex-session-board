import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import tarfile
import time
import tomllib
from pathlib import Path

from verify_acceptance import fingerprint
from package_migration import build_package


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"x86_64-unknown-linux-gnu", "aarch64-apple-darwin"}


def command(arguments, root=ROOT):
    return subprocess.check_output(arguments, cwd=root, text=True).strip()


def versions(root):
    cargo = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    locked = [item["version"] for item in lock["package"] if item["name"] == "codex-session-board"]
    values = [cargo, *locked, *(json.loads((root / name).read_text())["version"]
                              for name in ("plugin.json", ".codex-plugin/plugin.json"))]
    if len(values) != 4 or len(set(values)) != 1:
        raise ValueError("Cargo and plugin manifest versions must all match")
    return cargo


def hashes(directory):
    files = {}
    for path in sorted(directory.rglob("*")):
        if path.relative_to(directory).parts[0] == ".git":
            continue
        if path.is_symlink():
            raise ValueError("Plugin packages cannot contain symlinks")
        if path.is_file() and path.relative_to(directory).as_posix() != "RELEASE.json":
            files[path.relative_to(directory).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return files


def verify_package(directory, installed=False):
    manifest = json.loads((directory / "RELEASE.json").read_text())
    if manifest["schema_version"] != 1 or manifest["target"] not in TARGETS:
        raise ValueError("Unsupported plugin release")
    actual = hashes(directory)
    if installed:
        actual.pop(".codex-marketplace-install.json", None)
    if manifest["files"] != actual:
        raise ValueError("Plugin package file hashes differ")
    for name in ("bin/codex-session-board", "bin/run-plugin"):
        if not (directory / name).stat().st_mode & 0o111:
            raise ValueError("Plugin launchers must be executable")
    if manifest["binary_sha256"] != manifest["files"]["bin/codex-session-board"]:
        raise ValueError("Plugin binary identity differs")
    for name in ("plugin.json", ".codex-plugin/plugin.json"):
        if json.loads((directory / name).read_text())["version"] != manifest["version"]:
            raise ValueError("Plugin package version differs")
    return manifest


def preflight(root, destination):
    started = time.monotonic()
    version = versions(root)
    bindings = json.loads((root / "acceptance/bindings.json").read_text())
    before = fingerprint(root, bindings["fingerprint_paths"])
    destination.mkdir(parents=True, exist_ok=False)
    archive = destination / "SOURCE.tar.gz"
    result = build_package(root, archive)
    with tempfile.TemporaryDirectory(prefix="board-source-preflight-") as temporary:
        with tarfile.open(archive) as source:
            source.extractall(temporary, filter="data")
        relocated = Path(temporary) / "codex-session-board"
        if versions(relocated) != version:
            raise ValueError("Portable source versions differ")
        if fingerprint(relocated, bindings["fingerprint_paths"]) != before:
            raise ValueError("Portable source archive is missing implementation files")
        for name in ("scripts/check", "scripts/validate", "scripts/promote", "scripts/integrate"):
            if not (relocated / name).stat().st_mode & 0o111:
                raise ValueError(f"Portable entry point is not executable: {name}")
    if fingerprint(root, bindings["fingerprint_paths"]) != before:
        raise ValueError("Source changed during package preflight")
    result.update(passed=True, version=version, fingerprint=before,
                  seconds=time.monotonic() - started)
    (destination / "PREFLIGHT.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def build(root, target, destination=None):
    if command(["git", "status", "--porcelain", "--untracked-files=all"], root):
        raise ValueError("Build plugin releases from a clean, committed checkout")
    native = next(line[6:] for line in command(["rustc", "-vV"], root).splitlines()
                  if line.startswith("host: "))
    target = target or native
    if target not in TARGETS or target != native:
        raise ValueError("Build each release on its native supported platform")
    version = versions(root)
    source = command(["git", "rev-parse", "HEAD"], root)
    bindings = json.loads((root / "acceptance/bindings.json").read_text())
    build_sha = fingerprint(root, bindings["fingerprint_paths"])
    cache = Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target/dev")))
    cache = (cache if cache.is_absolute() else root / cache).resolve()
    subprocess.run(["cargo", "build", "--release", "--locked", "--offline", "--bin",
                    "codex-session-board", "--target", target, "--target-dir", str(cache)],
                   cwd=root, check=True)
    binary = cache / target / "release/codex-session-board"
    if command([str(binary), "--version"], root) != f"codex-session-board {version}":
        raise ValueError("Built binary version differs from source")
    if fingerprint(root, bindings["fingerprint_paths"]) != build_sha:
        raise ValueError("Source changed during release build")
    if destination is None:
        (root / "outputs").mkdir(exist_ok=True)
        destination = Path(tempfile.mkdtemp(prefix="codex-session-board-plugin.", dir=root / "outputs"))
    else:
        destination.mkdir(parents=True, exist_ok=False)
    (destination / "bin").mkdir()
    shutil.copy2(binary, destination / "bin/codex-session-board")
    shutil.copy2(root / "scripts/run-plugin", destination / "bin/run-plugin")
    (destination / "bin/run-plugin").chmod(0o755)
    for name in ("plugin.json", "mcp.json", ".mcp.json", "index.html"):
        shutil.copy2(root / name, destination / name)
    for name in (".codex-plugin", "assets", "skills", ".agents/plugins"):
        shutil.copytree(root / name, destination / name)
    build_package(root, destination / "SOURCE.tar.gz")
    manifest = {
        "schema_version": 1, "version": version, "target": target, "source_commit": source,
        "contract_sha256": hashlib.sha256((root / "acceptance/requirements.json").read_bytes()).hexdigest(),
        "build_sha256": build_sha, "files": hashes(destination),
    }
    manifest["binary_sha256"] = manifest["files"]["bin/codex-session-board"]
    (destination / "RELEASE.json").write_text(json.dumps(manifest, indent=2) + "\n")
    verify_package(destination)
    return destination


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", choices=sorted(TARGETS))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--preflight", action="store_true")
    arguments = parser.parse_args()
    if arguments.preflight:
        if arguments.output is None or arguments.target is not None:
            parser.error("--preflight requires --output and does not build a target")
        print(json.dumps(preflight(ROOT, arguments.output.resolve())))
        return
    print(build(ROOT, arguments.target, arguments.output.resolve() if arguments.output else None))


if __name__ == "__main__":
    main()
