import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts.verify_acceptance import fingerprint


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def identity():
    bindings = json.loads((ROOT / "acceptance/bindings.json").read_text())
    return {
        "commit": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}"),
        "fingerprint": fingerprint(ROOT, bindings["fingerprint_paths"]),
        "platform": sys.platform, "machine": platform.machine(),
    }


def related_commands(base):
    changed = git("diff", "--name-only", f"{base}...HEAD").splitlines()
    targets = {
        path.split("/")[1].split(".")[0] for path in changed
        if path.startswith("tests/") and not path.startswith("tests/support/")
    }
    arguments = ["cargo", "test", "--locked", "--offline"]
    if any(path.startswith("src/") or path in ("Cargo.toml", "Cargo.lock") for path in changed):
        arguments.append("--lib")
        targets.update(("board_contract", "board_io", "native_authority", "remote_sessions"))
    if any(path == "index.html" or path.endswith(".mjs") for path in changed):
        targets.add("ui_layout")
    if any(path.startswith("scripts/") and path.endswith(".rs") for path in changed):
        arguments.extend(["--example", "project_check"])
    if any(path in (".githooks/pre-commit", "scripts/check", "scripts/validate") for path in changed):
        targets.add("git_hooks")
    for target in sorted(targets):
        arguments.extend(["--test", target])
    commands = [arguments + ["--", "--test-threads=1"]] if len(arguments) > 5 else []
    if any(path.endswith(".py") for path in changed):
        commands.append(["python3", "-m", "unittest", "discover", "-s", "acceptance"])
    return commands


def execute(name, arguments, output):
    started = time.monotonic()
    print(f"[候选验证] {name}", flush=True)
    log = output / f"{name}.log"
    with log.open("w") as stream:
        process = subprocess.run(arguments, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
    return {
        "name": name, "arguments": arguments, "returncode": process.returncode,
        "seconds": time.monotonic() - started,
        "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--scope", choices=("pr", "candidate"), default="candidate")
    parser.add_argument("--base", default="origin/main")
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    if output.is_relative_to(ROOT):
        raise ValueError("验证记录必须放在源码之外")
    output.mkdir(parents=True, exist_ok=False)
    before = identity()
    record = {
        "schema_version": 1, "scope": arguments.scope, "source": before,
        "started_at": datetime.now(timezone.utc).isoformat(), "steps": [], "passed": False,
    }
    commands = [
        ["./scripts/check"],
        ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
    ]
    if arguments.scope == "candidate":
        commands.append([
            "python3", "scripts/verify_acceptance.py", "--scope", "automated",
            "--report", str(output / "automated.json"),
        ])
    else:
        commands.extend(related_commands(arguments.base))
    for number, command in enumerate(commands):
        step = execute(f"step-{number + 1}", command, output)
        record["steps"].append(step)
        (output / "VALIDATION.json").write_text(json.dumps(record, indent=2) + "\n")
        if step["returncode"]:
            print((output / f"{step['name']}.log").read_text()[-12000:], file=sys.stderr)
            return 1
    if identity() != before or git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("验证期间源码或提交身份发生变化")
    if arguments.scope == "candidate":
        automatic = json.loads((output / "automated.json").read_text())
        if not automatic["automated_passed"] or automatic["build_sha256"] != before["fingerprint"]:
            raise ValueError("自动报告与候选身份不一致")
        record["automated_sha256"] = hashlib.sha256((output / "automated.json").read_bytes()).hexdigest()
    record.update(passed=True, finished_at=datetime.now(timezone.utc).isoformat())
    (output / "VALIDATION.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"候选验证失败：{error}", file=sys.stderr)
        sys.exit(1)
