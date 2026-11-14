import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


ROOT = Path.cwd().resolve()


def command(arguments, timeout=600):
    try:
        result = subprocess.run(
            arguments, cwd=ROOT, capture_output=True, text=True, timeout=timeout,
            env={**os.environ, "RUST_BACKTRACE": "0",
                 **({"TMPDIR": "/private/tmp"} if sys.platform == "darwin" else {})},
        )
    except subprocess.TimeoutExpired as error:
        transcript = []
        for captured in (error.stdout, error.stderr):
            transcript.append(captured.decode(errors="replace") if isinstance(captured, bytes) else captured or "")
        raise TimeoutError(f"{error}\n" + "\n".join(transcript)[-10000:]) from error
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


def run_target(prefix, executable, expected):
    started = time.monotonic()
    threads = "2" if prefix in ("test::git_hooks", "example::project_check") else "1"
    timeout = 1200 if prefix == "test::git_hooks" else 600
    process = command([str(executable), f"--test-threads={threads}"], timeout=timeout)
    results = {}
    for line in process.stdout.splitlines():
        match = re.fullmatch(r"test (.+) \.\.\. (ok|FAILED|ignored)", line)
        if match:
            identifier = prefix + "::" + match[1]
            if identifier in results or identifier not in expected:
                raise ValueError("Duplicate or unexpected Rust test outcome")
            results[identifier] = {
                "id": identifier,
                "status": {"ok": "pass", "FAILED": "fail", "ignored": "skip"}[match[2]],
                "detail": "" if match[2] == "ok" else (process.stdout + process.stderr)[-6000:],
            }
    if set(results) != expected:
        raise ValueError("Missing Rust test outcomes: " + repr(sorted(expected - results.keys())))
    if process.returncode and all(record["status"] == "pass" for record in results.values()):
        raise ValueError("Rust test process failed despite passing outcomes")
    timing = {"group": prefix, "seconds": time.monotonic() - started,
              "test_count": len(results), "threads": int(threads), "returncode": process.returncode}
    print(json.dumps(timing), file=sys.stderr, flush=True)
    return results, timing


def run_rust(targets, names, selected=None):
    selected = set(names) if selected is None else selected
    groups = [(prefix, executable, {name for name in selected if names[name][0] == prefix})
              for prefix, executable in targets.items()]
    groups = [group for group in groups if group[2]]
    tools = {"test::git_hooks", "example::project_check"}
    results, timings = {}, []
    for group in groups:
        if group[0] not in tools:
            records, timing = run_target(*group)
            results.update(records)
            timings.append(timing)
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(run_target, *group) for group in groups if group[0] in tools]
        for future in futures:
            records, timing = future.result()
            results.update(records)
            timings.append(timing)
    return results, timings


def run(request, prefixes=None):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported adapter protocol")
    requested = request.get("tests")
    if not isinstance(requested, list) or not requested or len(requested) != len(set(requested)):
        raise ValueError("Expected distinct test identifiers")
    build_started = time.monotonic()
    targets = executables()
    timings = [{"group": "build", "seconds": time.monotonic() - build_started}]
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
    available = set(names) | set(python_names)
    if prefixes is not None and (
        not prefixes or any(not any(name.startswith(prefix + "::") for name in names)
                            for prefix in prefixes)
    ):
        raise ValueError("An integration test group is missing")
    selected = available if prefixes is None else {
        name for name in names if any(name.startswith(prefix + "::") for prefix in prefixes)
    }
    outcomes, rust_timings = run_rust(targets, names, selected & set(names))
    timings.extend(rust_timings)
    if prefixes is None:
        started = time.monotonic()
        python_result = python_tests({"protocol": 1, "tests": python_names})
        outcomes.update({item["id"]: item for item in python_result["results"]})
        timings.append({"group": "python", "seconds": time.monotonic() - started,
                        "test_count": len(python_names)})
    if set(outcomes) != selected:
        raise ValueError("Integration test results differ from the selected inventory")
    return {"protocol": 1, "results": list(outcomes.values()), "timings": timings,
            "inventory": sorted(available), "selected": sorted(selected),
            "not_run": sorted(available - selected)}


if __name__ == "__main__":
    try:
        print(json.dumps(run(json.load(sys.stdin)), ensure_ascii=False))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
