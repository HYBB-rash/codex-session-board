#!/usr/bin/env python3
"""Validate a generated project-context structure using only the standard library."""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import date
from pathlib import Path


DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
PLACEHOLDER_RE = re.compile(r"\{\{[^}]+\}\}")
SECRET_RE = re.compile(r"(?:sk-[A-Za-z0-9]{20,}|gh[pousr]_[A-Za-z0-9_]{20,})")


def load_json(path: Path, errors: list[str]) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append(f"missing: {path.relative_to(path.parents[1])}")
        return {}
    except json.JSONDecodeError as exc:
        errors.append(f"invalid JSON-compatible YAML: {path}: {exc}")
        return {}
    if not isinstance(value, dict):
        errors.append(f"expected object: {path}")
        return {}
    return value


def require(mapping: dict, key: str, label: str, errors: list[str]) -> object:
    if key not in mapping:
        errors.append(f"{label} missing key: {key}")
    return mapping.get(key)


def validate_date(value: object, label: str, errors: list[str]) -> None:
    if not isinstance(value, str) or not DATE_RE.fullmatch(value):
        errors.append(f"{label} must be YYYY-MM-DD: {value!r}")
        return
    try:
        date.fromisoformat(value)
    except ValueError:
        errors.append(f"{label} is not a calendar date: {value!r}")


def validate_paths(root: Path, manifest: dict, errors: list[str]) -> None:
    paths = manifest.get("paths", {})
    if not isinstance(paths, dict):
        errors.append("PROJECT.paths must be an object")
        return
    for field in ("source_roots", "config_paths"):
        values = paths.get(field, [])
        if not isinstance(values, list):
            errors.append(f"PROJECT.paths.{field} must be a list")
            continue
        for raw in values:
            if not isinstance(raw, str):
                errors.append(f"PROJECT.paths.{field} contains a non-string path")
                continue
            candidate = (root / raw).resolve()
            try:
                candidate.relative_to(root)
            except ValueError:
                errors.append(f"path escapes project root: {raw}")


def scan_files(root: Path, errors: list[str]) -> None:
    for path in root.glob("context/**/*"):
        if not path.is_file() or path.name == "README.md":
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if PLACEHOLDER_RE.search(text):
            errors.append(f"unresolved placeholder: {path.relative_to(root)}")
        if SECRET_RE.search(text):
            errors.append(f"possible credential in: {path.relative_to(root)}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    errors: list[str] = []
    manifest = load_json(root / "context" / "PROJECT.yaml", errors)
    state = load_json(root / "context" / "STATE.yaml", errors)

    for key in ("schema_version", "template_version", "project", "profile", "locale", "paths", "verification"):
        require(manifest, key, "PROJECT", errors)
    concurrency = require(manifest, "concurrency", "PROJECT", errors)
    for key in ("schema_version", "updated_at", "current_phase", "active_tasks", "last_completed_task", "verification"):
        require(state, key, "STATE", errors)

    profile = manifest.get("profile")
    if profile not in {"generic", "code", "ops"}:
        errors.append(f"unsupported profile: {profile!r}")
    locale = manifest.get("locale", {})
    if isinstance(locale, dict) and locale.get("date_policy") != "local_calendar":
        errors.append("PROJECT.locale.date_policy must be local_calendar")

    if not isinstance(concurrency, dict):
        errors.append("PROJECT.concurrency must be an object")
    else:
        mode = concurrency.get("default_mode")
        if mode not in {"isolated_write", "serial_write"}:
            errors.append("PROJECT.concurrency.default_mode must be isolated_write or serial_write")
        for key in ("coordinator_owned", "worker_allowed_records"):
            value = concurrency.get(key)
            if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
                errors.append(f"PROJECT.concurrency.{key} must be a list of strings")
        coordinator_owned = concurrency.get("coordinator_owned")
        if isinstance(coordinator_owned, list) and all(
            isinstance(item, str) and item for item in coordinator_owned
        ):
            required_owned = {
                "context/STATE.yaml",
                "current_phase",
                "verification",
                "context/journal/",
                "task_registration",
            }
            missing_owned = sorted(required_owned.difference(coordinator_owned))
            for item in missing_owned:
                errors.append(
                    f"PROJECT.concurrency.coordinator_owned missing required entry: {item}"
                )

    updated_at = state.get("updated_at")
    validate_date(updated_at, "STATE.updated_at", errors)
    phase = state.get("current_phase", {})
    if not isinstance(phase, dict):
        errors.append("STATE.current_phase must be an object")
        phase = {}
    validate_date(phase.get("as_of"), "STATE.current_phase.as_of", errors)
    if phase.get("as_of") != updated_at:
        errors.append("STATE.current_phase.as_of must equal STATE.updated_at")

    active = state.get("active_tasks")
    if not isinstance(active, list):
        errors.append("STATE.active_tasks must be a list")
    if state.get("last_completed_task") is not None and not isinstance(state.get("last_completed_task"), str):
        errors.append("STATE.last_completed_task must be null or a string")

    verification = state.get("verification", {})
    if not isinstance(verification, dict):
        errors.append("STATE.verification must be an object")
        verification = {}
    status = verification.get("status")
    if status not in {"pending", "passed"}:
        errors.append("STATE.verification.status must be pending or passed")
    if status == "pending" and verification.get("last_checked_at") is not None:
        errors.append("pending verification must not have last_checked_at")
    if status == "passed":
        validate_date(verification.get("last_checked_at"), "STATE.verification.last_checked_at", errors)
        if isinstance(updated_at, str) and isinstance(verification.get("last_checked_at"), str):
            if verification["last_checked_at"] < updated_at:
                errors.append("verification.last_checked_at must not precede STATE.updated_at")

    validate_paths(root, manifest, errors)
    scan_files(root, errors)

    required_files = [
        "AGENTS.md",
        "context/README.md",
        "context/results/README.md",
        "scripts/verify_context.sh",
        "scripts/check_context.py",
    ]
    for relative in required_files:
        if not (root / relative).is_file():
            errors.append(f"missing required file: {relative}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("context check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
