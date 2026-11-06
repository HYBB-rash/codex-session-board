import json
import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path.cwd().resolve()


def command(arguments, timeout=600):
    result = subprocess.run(
        arguments, cwd=ROOT, capture_output=True, text=True, timeout=timeout,
        env={**os.environ, "RUST_BACKTRACE": "0"},
    )
    return result


def executables():
    process = command([
        "cargo", "test", "--all-targets", "--locked", "--offline",
        "--no-run", "--message-format=json",
    ])
    if process.returncode:
        raise ValueError(process.stderr[-6000:])
    targets = {}
    for line in process.stdout.splitlines():
        item = json.loads(line)
        if item.get("executable") and item.get("profile", {}).get("test"):
            target = item["target"]
            prefix = target["kind"][0] + "::" + target["name"]
            targets[prefix] = Path(item["executable"])
    return targets


def inventory(targets):
    names = {}
    for prefix, executable in targets.items():
        process = command([str(executable), "--list"])
        if process.returncode:
            raise ValueError(process.stdout + process.stderr)
        for line in process.stdout.splitlines():
            if line.endswith(": test"):
                name = line.removesuffix(": test")
                names[prefix + "::" + name] = (prefix, name)
    if not names:
        raise ValueError("Rust test inventory is empty")
    return names


def run_rust(targets, names):
    results = {}
    for prefix, executable in targets.items():
        expected = {name for name, (target, _) in names.items() if target == prefix}
        if not expected:
            continue
        process = command([str(executable), "--test-threads=1"])
        for line in process.stdout.splitlines():
            match = re.fullmatch(r"test (.+) \.\.\. (ok|FAILED|ignored)", line)
            if match:
                identifier = prefix + "::" + match[1]
                if identifier in results:
                    raise ValueError("Duplicate Rust test outcome")
                results[identifier] = {
                    "id": identifier,
                    "status": {"ok": "pass", "FAILED": "fail", "ignored": "skip"}[match[2]],
                    "detail": "" if match[2] == "ok" else (process.stdout + process.stderr)[-6000:],
                }
        if not expected.issubset(results):
            raise ValueError("Missing Rust test outcomes: " + repr(sorted(expected - results.keys())))
        if process.returncode and all(results[name]["status"] == "pass" for name in expected):
            raise ValueError("Rust test process failed despite passing outcomes")
    return results


def run(request):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported adapter protocol")
    requested = request.get("tests")
    if not isinstance(requested, list) or not requested or len(requested) != len(set(requested)):
        raise ValueError("Expected distinct test identifiers")
    targets = executables()
    names = inventory(targets)
    sys.path.insert(0, str(ROOT))
    import unittest
    from acceptance.unittest_adapter import test_identifiers, run as python_tests
    suite = unittest.defaultTestLoader.discover(str(ROOT / "acceptance"), top_level_dir=str(ROOT))
    python_names = list(test_identifiers(suite))
    if set(requested) != set(names) | set(python_names):
        raise ValueError("Test inventory differs from bindings: " + repr(sorted(
            set(requested).symmetric_difference(set(names) | set(python_names))
        )))
    outcomes = run_rust(targets, names)
    python_result = python_tests({"protocol": 1, "tests": python_names})
    outcomes.update({item["id"]: item for item in python_result["results"]})
    return {"protocol": 1, "results": list(outcomes.values())}


if __name__ == "__main__":
    try:
        print(json.dumps(run(json.load(sys.stdin)), ensure_ascii=False))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
