import hashlib
import json


TARGETS = {"x86_64-unknown-linux-gnu", "aarch64-apple-darwin"}


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
    if not isinstance(manifest, dict):
        raise ValueError("Plugin release manifest must be an object")
    if manifest.get("schema_version") != 1 or not isinstance(manifest.get("target"), str) or manifest["target"] not in TARGETS:
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
        metadata = json.loads((directory / name).read_text())
        if not isinstance(metadata, dict) or metadata.get("version") != manifest["version"]:
            raise ValueError("Plugin package version differs")
    return manifest
