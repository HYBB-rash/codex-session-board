import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
ROOT = Path.cwd().resolve()


def command(arguments, timeout=600, input=None):
    try:
        result = subprocess.run(
            arguments, cwd=ROOT, capture_output=True, text=True, timeout=timeout, input=input,
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


def python_inventory():
    process = command([sys.executable, "-c",
        "import json,unittest\nfrom pathlib import Path\n"
        "from acceptance.unittest_adapter import test_identifiers\n"
        "suite=unittest.defaultTestLoader.discover(str(Path.cwd()/'acceptance'),top_level_dir=str(Path.cwd()))\n"
        "print(json.dumps(list(test_identifiers(suite))))"])
    if process.returncode:
        raise ValueError(process.stderr)
    names = json.loads(process.stdout)
    if not names or len(names) != len(set(names)):
        raise ValueError("Python test inventory is empty or duplicated")
    return names


def run_python(names):
    process = command([sys.executable, str(ROOT / "acceptance/unittest_adapter.py")],
                      input=json.dumps({"protocol": 1, "tests": names}))
    if process.returncode:
        raise ValueError(process.stderr)
    from scripts.verify_acceptance import validate_results
    return validate_results(json.loads(process.stdout), set(names))


def run(request, prefixes=None, include_python=False):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported adapter protocol")
    requested = request.get("tests")
    if not isinstance(requested, list) or not requested or len(requested) != len(set(requested)):
        raise ValueError("Expected distinct test identifiers")
    build_started = time.monotonic()
    targets = executables()
    timings = [{"group": "build", "seconds": time.monotonic() - build_started}]
    names = inventory(targets)
    python_names = python_inventory()
    if set(requested) != set(names) | set(python_names):
        raise ValueError("Test inventory differs from bindings: " + repr(sorted(
            set(requested).symmetric_difference(set(names) | set(python_names))
        )))
    available = set(names) | set(python_names)
    if prefixes is not None and (
        not prefixes and not include_python
        or any(not any(name.startswith(prefix + "::") for name in names)
                            for prefix in prefixes)
    ):
        raise ValueError("An integration test group is missing")
    selected = available if prefixes is None else {
        name for name in names if any(name.startswith(prefix + "::") for prefix in prefixes)
    }
    if include_python:
        selected.update(python_names)
    outcomes, rust_timings = run_rust(targets, names, selected & set(names))
    timings.extend(rust_timings)
    if prefixes is None or include_python:
        started = time.monotonic()
        outcomes.update(run_python(python_names))
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
