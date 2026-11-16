import hashlib
import json
import re

from verify_package_startup import CHECKS


CASES = {
    "success": None,
    "mcp-config-drift": "cannot read",
    "mcp-failure-exit": "unsuccessfully",
    "wrong-backend": "不是启动本看板",
    "service-failure-exit": "提前退出",
    "wrong-sidebar": "侧栏不属于",
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_digests(root):
    paths = [
        "scripts/package_scenarios.py", "scripts/verify_package_startup.py",
        "scripts/pr_connection_probe.py", "scripts/release_scenarios.py",
        "examples/dev_fixture.rs", "Cargo.lock",
        *(path.relative_to(root).as_posix() for path in sorted((root / "examples/dev_fixture").glob("*.rs"))),
    ]
    return {name: digest(root / name) for name in paths}


def zero_bytes(value):
    return isinstance(value, list) and bool(value) and all(type(count) is int and count == 0 for count in value)


def validate(record, manifest, archive, startup, root, fixture=None):
    if (not isinstance(record, dict) or record.get("schema_version") != 1
            or record.get("scope") != "pr_package_scenarios" or record.get("passed") is not True
            or record.get("manifest") != manifest
            or record.get("package_asset_sha256") != digest(archive)
            or record.get("startup_sha256") != digest(startup)
            or record.get("source_digests") != source_digests(root)):
        raise ValueError("Saved scenarios do not match the package, startup or verifier source")
    fixture_sha = record.get("fixture_sha256", "")
    if (not isinstance(fixture_sha, str) or re.fullmatch("[0-9a-f]{64}", fixture_sha) is None
            or fixture is not None and fixture_sha != digest(fixture)
            or record.get("verifier_sha256") != digest(root / "scripts/verify_package_startup.py")):
        raise ValueError("Saved scenarios have a different fixture or startup verifier digest")
    cases = record.get("cases", [])
    reports = record.get("reports", {})
    if (not isinstance(cases, list) or len(cases) != len(CASES)
            or any(not isinstance(case, dict) for case in cases)
            or {case.get("name") for case in cases} != set(CASES)
            or not isinstance(reports, dict) or set(reports) != set(CASES)):
        raise ValueError("Saved scenarios must contain all six distinct package cases")
    identity = ("source_commit", "build_sha256", "binary_sha256", "version", "target")
    for case in cases:
        name = case["name"]
        report = reports[name]
        rejected = CASES[name] is not None
        if (case.get("passed") is not True or case.get("cleanup_confirmed") is not True
                or case.get("expected_rejection") is not rejected
                or not isinstance(report, dict) or report.get("cleanup_confirmed") is not True):
            raise ValueError(f"Saved scenario did not pass with confirmed cleanup: {name}")
        if name == "wrong-sidebar":
            if (report.get("passed") is not True or type(report.get("status")) is not int
                    or report["status"] < 400 or not zero_bytes(report.get("bytes_per_connection"))
                    or CASES[name] not in json.dumps(report.get("response"), ensure_ascii=False)):
                raise ValueError("Saved sidebar scenario did not reject the foreign peer")
        elif (any(report.get(field) != manifest.get(field) or not manifest.get(field) for field in identity)
              or report.get("schema_version") != 1 or report.get("scope") != "package_startup"
              or report.get("passed") is not (not rejected)
              or case.get("actual_startup_passed") is not (not rejected)
              or rejected and CASES[name] not in report.get("error", "")
              or not rejected and report.get("checks") != CHECKS):
            raise ValueError(f"Saved scenario has wrong package identity or expected rejection: {name}")
    if not zero_bytes(record.get("wrong_backend_bytes")):
        raise ValueError("Saved backend scenario did not confirm zero foreign RPC bytes")
    return record


def collect(project, manifest, archive, fixture, output, startup):
    record = json.loads((output / "SCENARIOS.json").read_text())
    record.update(
        schema_version=1, package_asset_sha256=digest(archive),
        startup_sha256=digest(startup), source_digests=source_digests(project),
        reports={name: json.loads((output / f"{name}.json").read_text()) for name in CASES},
    )
    for case in record["cases"]:
        case["report"] = f"reports/{case['name']}"
        if case["name"] == "wrong-sidebar":
            case.setdefault("expected_rejection", True)
    validate(record, manifest, archive, startup, project, fixture)
    return record
