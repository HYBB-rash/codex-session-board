import re
from pathlib import PurePosixPath


CONTENTS = "delivery-archive-contents"
EXCLUSIONS = "delivery-archive-exclusions"
INTEGRITY = "delivery-archive-integrity"
DELIVERY_CHECKS = {CONTENTS, EXCLUSIONS, INTEGRITY}

REQUIRED_FILES = {
    ".codex-plugin/plugin.json", ".mcp.json", "README.md", "INSTALL.md",
    "acceptance/model.py", "acceptance/requirements.json", "acceptance/contract.lock.json",
    "acceptance/candidate-map.json", "acceptance/gate-bindings.json",
    "acceptance/check-candidate.mjs", "acceptance/verify-typescript.py",
    "acceptance/public-operations.test.mjs", "acceptance/release_gate.py",
    "acceptance/release_identity.py", "acceptance/delivery_checks.py",
    "scripts/package-install.md", "dist/bin/codex-enhancer.js",
    "dist/bin/codex-session-board.js", "dist/runtime/main.js", "dist/runtime/preload.js",
    "dist/board-assets/board-operations.json", "dist/board-assets/board.js",
    "dist/board-assets/board.svg", "dist/board-assets/index.html",
    "dist/board-assets/manifest.webmanifest", "dist/board-assets/mcp.html",
    "dist/board-assets/sw.js", "dist/board-assets/version.txt",
    "dist/compat/entrypoints.test.js", "dist/migration/legacy.test.js",
    "dist/migration/legacy.js",
}
EVIDENCE_DOCS = {"session-board-native-checkpoint.md", "session-board-typescript-acceptance.md"}
FORBIDDEN_PATHS = {
    "state", "logs", "screenshots", "node_modules", "cache", "caches", ".cache", "__pycache__",
    "codex-home", "user-data", ".git", "config.json", "records.json", "ledger.json",
    "server.json", "credentials.json", "scan-progress.json",
}
FORBIDDEN_FILE = re.compile(
    r"(?:^\.env(?:\.|$)|\.sqlite[23]?(?:-(?:wal|shm|journal))?$|\.db(?:-(?:wal|shm|journal))?$"
    r"|\.(?:jsonl|ndjson|log|pyc|pyo|png|jpe?g|webp)$)", re.IGNORECASE,
)


def unexpected_entry(name):
    parts = PurePosixPath(name).parts
    if set(piece.lower() for piece in parts) & FORBIDDEN_PATHS or FORBIDDEN_FILE.search(parts[-1]):
        return True
    if len(parts) == 1:
        return name not in {
            "acceptance", "scripts", "src", "ui", "dist", "docs", ".codex-plugin", ".mcp.json",
            "README.md", "INSTALL.md", "release.json", "package.json", "package-lock.json",
            "tsconfig.json", "biome.json",
        }
    if parts[0] == ".codex-plugin":
        return name != ".codex-plugin/plugin.json"
    if parts[0] == "docs":
        return len(parts) != 2 or parts[1] not in EVIDENCE_DOCS
    return parts[0] not in {"acceptance", "scripts", "src", "ui", "dist"} or any(
        part.startswith(".") for part in parts[1:]
    )


def delivery_checks(archive_path, expected_digest, files, members, documented_migration):
    missing = sorted(REQUIRED_FILES - files.keys())
    content_issues = []
    if missing:
        content_issues.append("缺少 " + ", ".join(missing))
    if files.get("INSTALL.md") != files.get("scripts/package-install.md"):
        content_issues.append("随包安装说明与源说明不一致")
    if not documented_migration:
        content_issues.append("随包说明缺少迁移和回退命令")
    unexpected = sorted(name for name in members if unexpected_entry(name))
    receipt = archive_path.with_name(archive_path.name + ".sha256")
    expected = f"{expected_digest}  {archive_path.name}"
    try:
        checksum_ok = (
            receipt.is_file() and not receipt.is_symlink()
            and receipt.read_text(encoding="utf-8").rstrip("\n") == expected
        )
    except (OSError, UnicodeError):
        checksum_ok = False
    return {
        CONTENTS: "。".join(content_issues),
        EXCLUSIONS: "归档含有意外或禁止的路径 " + ", ".join(unexpected) if unexpected else "",
        INTEGRITY: "" if checksum_ok else "缺少匹配的发行 SHA256 清单 " + receipt.name,
    }
