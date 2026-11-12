import argparse
import fnmatch
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
KINDS = {"automated", "mixed", "live"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def inside(root, path):
    target = path.resolve()
    target.relative_to(root.resolve())
    return target


def fingerprint_files(root, patterns):
    if not isinstance(patterns, list) or not patterns:
        raise ValueError("fingerprint_paths must be a nonempty list")
    paths = set()
    for pattern in patterns:
        if not isinstance(pattern, str) or Path(pattern).is_absolute() or ".." in Path(pattern).parts:
            raise ValueError("Fingerprint paths must stay inside the project")
        for matched in root.glob(pattern):
            candidates = matched.rglob("*") if matched.is_dir() else [matched]
            for path in candidates:
                if "__pycache__" in path.parts or path.suffix == ".pyc":
                    continue
                if path.is_file():
                    inside(root, path)
                    paths.add(path)
    if not paths:
        raise ValueError("The implementation fingerprint cannot be empty")
    return sorted(paths)


def fingerprint(root, patterns):
    hasher = hashlib.sha256()
    for path in fingerprint_files(root, patterns):
        hasher.update(path.relative_to(root).as_posix().encode() + b"\0")
        hasher.update(bytes.fromhex(digest(path)))
    return hasher.hexdigest()


def same_file(first, second):
    return first.resolve() == second.resolve() or (
        first.exists() and second.exists() and first.samefile(second)
    )


def validate_report_path(root, contract_path, bindings_path, evidence_path, report_path):
    patterns = read_json(bindings_path)["fingerprint_paths"]
    lock_path = contract_path.with_name("contract.lock.json")
    protected = [
        contract_path, bindings_path, lock_path, *fingerprint_files(root, patterns),
        *(contract_path.parent / name for name in read_json(lock_path)["files"]),
    ]
    if evidence_path:
        protected.append(evidence_path)
        for item in read_json(evidence_path).get("checks", {}).values():
            protected.extend(
                evidence_path.parent / artifact["path"]
                for artifact in item.get("artifacts", [])
            )
    if any(same_file(report_path, path) for path in protected):
        raise ValueError("Report output would overwrite verification inputs or evidence")
    try:
        relative = report_path.resolve().relative_to(root.resolve())
    except ValueError:
        return
    for pattern in patterns:
        candidate = root / pattern
        if candidate.is_dir() and candidate.resolve() in report_path.resolve().parents:
            raise ValueError("Report output cannot enter fingerprinted source directories")
        if fnmatch.fnmatchcase(relative.as_posix(), pattern):
            raise ValueError("Report output would become a fingerprinted input")


def requirement_tests(mapping):
    branches = mapping if isinstance(mapping, dict) else {"common": mapping}
    if isinstance(mapping, dict) and set(branches) != {"common", "linux", "darwin"}:
        raise ValueError("Platform mappings require common, linux and darwin branches")
    for names in branches.values():
        if (not isinstance(names, list)
                or not all(isinstance(name, str) and name.strip() == name and name for name in names)
                or len(names) != len(set(names))):
            raise ValueError("Invalid test mapping")
    if isinstance(mapping, dict) and sys.platform not in branches:
        raise ValueError("Unsupported acceptance platform: " + sys.platform)
    common = branches["common"]
    specific = branches.get(sys.platform, [])
    if any(set(common) & set(names) for key, names in branches.items() if key != "common"):
        raise ValueError("Platform tests duplicate common tests")
    return common + specific


def validate_contract(contract, bindings):
    if contract.get("schema_version") != 1 or bindings.get("schema_version") != 1:
        raise ValueError("Unsupported contract or bindings version")
    requirements = contract.get("requirements")
    if not isinstance(requirements, list) or not requirements:
        raise ValueError("The contract must contain requirements")
    identifiers = []
    for item in requirements:
        identifier = item["id"]
        if not isinstance(identifier, str) or not identifier:
            raise ValueError("Invalid requirement identifier")
        identifiers.append(identifier)
        if item.get("verification") not in KINDS:
            raise ValueError(f"Unknown verification kind: {identifier}")
        for field in ("title", "given", "when"):
            if not isinstance(item.get(field), str) or not item[field]:
                raise ValueError(f"Missing {field}: {identifier}")
        if not isinstance(item.get("then"), list) or not item["then"]:
            raise ValueError(f"Missing expected behavior: {identifier}")
    if len(identifiers) != len(set(identifiers)):
        raise ValueError("Duplicate requirement identifiers")
    mappings = bindings.get("requirements", {})
    if set(mappings) != set(identifiers):
        raise ValueError("Bindings must cover exactly the contract identifiers")
    tests = set()
    for item in requirements:
        names = requirement_tests(mappings[item["id"]])
        mapping = mappings[item["id"]]
        empty_platform = isinstance(mapping, dict) and any(
            not mapping["common"] and not mapping[platform] for platform in ("linux", "darwin"))
        if item["verification"] != "live" and (not names or empty_platform):
            raise ValueError(f"No automated checks bound to {item['id']}")
        tests.update(names)
    if not tests:
        raise ValueError("No automated checks to execute")
    return sorted(tests)


def validate_results(payload, requested):
    if payload.get("protocol") != 1 or not isinstance(payload.get("results"), list):
        raise ValueError("Invalid test-adapter response")
    records = {}
    for item in payload["results"]:
        identifier = item["id"]
        if identifier in records or identifier not in requested:
            raise ValueError("Duplicate or unexpected test result")
        if item.get("status") not in {"pass", "fail", "error", "skip"}:
            raise ValueError("Unknown test result status")
        records[identifier] = item
    if set(records) != set(requested):
        raise ValueError("Missing test results")
    return records


def execute_adapter(root, bindings, tests):
    adapter = bindings["adapter"]
    command = adapter["command"]
    if not isinstance(command, list) or not command or not all(isinstance(part, str) for part in command):
        raise ValueError("The adapter command must be an argument array")
    command = [sys.executable if part == "{python}" else part for part in command]
    timeout = adapter.get("timeout_seconds", 120)
    if not isinstance(timeout, (int, float)) or not 0 < timeout <= 3600:
        raise ValueError("Invalid adapter timeout")
    process = subprocess.run(
        command, input=json.dumps({"protocol": 1, "tests": tests}),
        capture_output=True, text=True, cwd=root, timeout=timeout, check=False,
    )
    if process.returncode:
        raise ValueError(
            f"Test adapter exited {process.returncode}: {process.stderr[-3000:]}"
        )
    return validate_results(json.loads(process.stdout), set(tests))


def live_result(identifier, evidence, evidence_path, contract_sha, build_sha):
    if (
        evidence.get("contract_sha256") != contract_sha
        or evidence.get("build_sha256") != build_sha
    ):
        return "pending", "缺少与当前需求和实现匹配的真实环境证据"
    item = evidence.get("checks", {}).get(identifier, {})
    if item.get("status") == "fail":
        return "fail", item.get("notes") or "真实环境验收失败"
    if item.get("status") != "pass":
        return "pending", "真实环境项目尚未验收"
    for field in ("operator", "environment", "notes"):
        if not isinstance(item.get(field), str) or not item[field].strip():
            return "pending", f"真实环境证据缺少 {field}"
    try:
        observed = datetime.fromisoformat(item["observed_at"])
        if observed.tzinfo is None or observed > datetime.now(timezone.utc):
            raise ValueError("Invalid observation time")
        artifacts = item["artifacts"]
        if not isinstance(artifacts, list) or not artifacts:
            raise ValueError("No artifacts")
        for artifact in artifacts:
            path = evidence_path.parent / artifact["path"]
            if not path.is_file() or digest(path) != artifact["sha256"]:
                raise ValueError("Missing or mismatching evidence artifact")
    except (KeyError, OSError, TypeError, ValueError):
        return "pending", "真实环境证据的时间或附件校验无效"
    return "pass", "真实环境证据齐全，内容仍须审阅"


def verify(root, contract_path, bindings_path, scope, evidence_path=None):
    contract = read_json(contract_path)
    bindings = read_json(bindings_path)
    tests = validate_contract(contract, bindings)
    lock = read_json(contract_path.with_name("contract.lock.json"))
    for relative, expected in lock["files"].items():
        if digest(inside(contract_path.parent, contract_path.parent / relative)) != expected:
            raise ValueError(f"The accepted contract baseline changed: {relative}")
    if "requirements.json" not in lock["files"] or "model.py" not in lock["files"]:
        raise ValueError("Contract lock must protect requirements and the reference model")
    contract_sha = digest(contract_path)
    if lock["files"].get(contract_path.name) != contract_sha:
        raise ValueError("The selected contract is not protected by its lock")
    build_sha = fingerprint(root, bindings["fingerprint_paths"])
    evidence = read_json(evidence_path) if evidence_path else {}
    records = execute_adapter(root, bindings, tests)
    if fingerprint(root, bindings["fingerprint_paths"]) != build_sha:
        raise ValueError("Implementation files changed during verification")
    items = []
    for requirement in contract["requirements"]:
        identifier = requirement["id"]
        bound = requirement_tests(bindings["requirements"][identifier])
        failed = [name for name in bound if records[name]["status"] != "pass"]
        status = "fail" if failed else "pass"
        detail = "自动检查通过" if bound else "需要真实环境验收"
        live_status = None
        if failed:
            detail = "自动检查失败或被跳过: " + ", ".join(failed)
        if requirement["verification"] in {"mixed", "live"}:
            live_status, live_detail = live_result(
                identifier, evidence, evidence_path, contract_sha, build_sha
            )
            if not failed:
                status, detail = live_status, live_detail
        items.append({
            "id": identifier, "title": requirement["title"], "status": status,
            "detail": detail, "automated_tests": bound, "live_status": live_status,
        })
    automated_ok = all(item["status"] == "pass" for item in records.values())
    complete = all(item["status"] == "pass" for item in items)
    passed = automated_ok if scope == "automated" else complete
    report = {
        "schema_version": 1, "contract_id": contract["contract_id"],
        "contract_sha256": contract_sha, "build_sha256": build_sha,
        "platform": sys.platform, "scope": scope, "passed": passed, "release_ready": complete,
        "automated_passed": automated_ok, "test_count": len(records),
        "requirements": items, "tests": list(records.values()),
    }
    exit_code = 0 if passed else 1 if any(item["status"] == "fail" for item in items) else 2
    return report, exit_code


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--contract", type=Path)
    parser.add_argument("--bindings", type=Path)
    parser.add_argument("--scope", choices=("release", "automated"), default="release")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    contract_path = args.contract or root / "acceptance/requirements.json"
    bindings_path = args.bindings or root / "acceptance/bindings.json"
    safe_report = False
    try:
        if args.report:
            validate_report_path(
                root, contract_path, bindings_path, args.evidence, args.report
            )
        safe_report = True
        report, exit_code = verify(
            root, contract_path, bindings_path, args.scope, args.evidence,
        )
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        report = {
            "schema_version": 1, "scope": args.scope, "passed": False,
            "release_ready": False, "error": str(error),
        }
        exit_code = 1
    if args.report and safe_report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    if "error" in report:
        print("ERROR:", report["error"])
    else:
        for item in report["requirements"]:
            print(f"{item['status'].upper():7} {item['id']:15} {item['title']} | {item['detail']}")
        print(json.dumps({
            key: report[key] for key in (
                "scope", "passed", "release_ready", "automated_passed",
                "test_count", "contract_sha256", "build_sha256",
            )
        }, ensure_ascii=False))
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
