import argparse
import gzip
import hashlib
import io
import json
import tarfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PATTERNS = (
    "requirements-delivery.txt",
    "Cargo.toml", "Cargo.lock", "build.rs", "clippy.toml", "project-rules.json",
    "src/**/*.rs", "tests/**/*.rs", "examples/**/*.rs", "index.html", "config.example.json",
    "plugin.json", "mcp.json", ".mcp.json", ".codex-plugin/plugin.json",
    ".agents/plugins/marketplace.json", ".gitignore", ".githooks/*",
    "AGENTS.md", "README.md", "Docs/*.md", "assets/*", "skills/**/*",
    "scripts/*.py", "scripts/*.rs", "scripts/*.mjs", "scripts/check", "scripts/install-git-hooks",
    "scripts/build-plugin", "scripts/run-plugin", "scripts/update-plugin", "scripts/release", ".github/workflows/*.yml",
    "scripts/dev", "scripts/dev_environment/*.py", "scripts/validate", "scripts/promote",
    "scripts/verify_context.sh", "context/**/*.md", "context/*.yaml",
    "acceptance/*.py", "acceptance/*.json", "acceptance/*.md",
)
FORBIDDEN_SUFFIXES = {".sqlite", ".sqlite3", ".db", ".log", ".png", ".pyc"}


def package_contents(root):
    contents = {}
    for pattern in PATTERNS:
        for path in root.glob(pattern):
            if not path.is_file():
                continue
            if path.is_symlink() or path.suffix in FORBIDDEN_SUFFIXES:
                raise ValueError(f"Not a portable source file: {path}")
            path.resolve().relative_to(root.resolve())
            contents[path.relative_to(root).as_posix()] = path.read_bytes()
    required = (
        "src/main.rs", "src/store.rs", "src/mcp.rs", "Cargo.lock", "build.rs", "Docs/MIGRATION.md",
        "acceptance/requirements.json", "acceptance/model.py",
        "acceptance/bindings.json", "acceptance/contract.lock.json",
        "acceptance/README.md", "assets/app-bridge.js", "context/PROJECT.yaml",
        "context/STATE.yaml", "scripts/verify_context.sh", "scripts/check_context.py",
    )
    if any(name not in contents for name in required):
        raise ValueError("Missing a required migration file")
    version = json.loads(contents["plugin.json"])["version"]
    if json.loads(contents[".codex-plugin/plugin.json"])["version"] != version:
        raise ValueError("Plugin manifest versions differ")
    manifest = {
        "schema_version": 1, "plugin_version": version,
        "files": {name: hashlib.sha256(body).hexdigest() for name, body in sorted(contents.items())},
    }
    contents["PACKAGE-MANIFEST.json"] = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode()
    return contents


def build_package(root, output):
    contents = package_contents(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as target, gzip.GzipFile(
        filename="", mode="wb", fileobj=target, mtime=0
    ) as compressed, tarfile.open(fileobj=compressed, mode="w") as archive:
        for name, body in sorted(contents.items()):
            entry = tarfile.TarInfo("codex-session-board/" + name)
            entry.size = len(body)
            entry.mode = 0o755 if name.startswith((".githooks/", "scripts/")) and "." not in Path(name).name else 0o644
            archive.addfile(entry, io.BytesIO(body))
    checksum = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_name(output.name + ".sha256").write_text(f"{checksum}  {output.name}\n")
    return {"archive": str(output), "sha256": checksum, "files": len(contents)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    print(json.dumps(build_package(arguments.root.resolve(), arguments.output.resolve())))
