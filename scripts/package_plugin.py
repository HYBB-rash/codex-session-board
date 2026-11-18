import argparse
import hashlib
from importlib.metadata import distribution
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
from plugin_package import TARGETS, hashes, verify_package


ROOT = Path(__file__).resolve().parents[1]


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


def bundle_updater(root, destination):
    dependency = distribution("websocket-client")
    if f"websocket-client=={dependency.version}" not in (root / "requirements-delivery.txt").read_text().splitlines():
        raise ValueError("Install the pinned requirements-delivery.txt before building a plugin")
    destination.mkdir()
    for name in ("board_update_job.py", "update_plugin.py", "plugin_package.py", "reload_plugins.py",
                 "desktop_identity.py"):
        shutil.copy2(root / "scripts" / name, destination / name)
    for path in dependency.files:
        if path.parts[0] == "websocket" and not {"__pycache__", "tests"}.intersection(path.parts):
            bundled = destination / path
            bundled.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path.locate(), bundled)
        elif path.name == "LICENSE":
            shutil.copy2(path.locate(), destination / "websocket-client-LICENSE")


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
        for name in ("scripts/check", "scripts/validate", "scripts/promote", "scripts/integrate", "scripts/pr-task"):
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
    bundle_updater(root, destination / "updater")
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
