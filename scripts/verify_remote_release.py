import argparse
import hashlib
import json
import subprocess
import sys
import uuid
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts.verify_acceptance import (
    digest, fingerprint, inside, live_result, same_file, validate_contract,
    validate_report_path, validate_results,
)


CASES = {
    "N5-STATE": "native5",
    "N5-EXTERNAL-CONFLICT": "native5",
    "N5-RUNTIME-DEDUPE": "native5",
    "N5-RECOVERY": "native5",
    "N5-UI": "native5",
    "R-CANONICAL-ACTIVE-OWNER": "remote",
    "R-RUNTIME-PHASES": "remote",
    "R-DISCOVERY": "remote",
    "R-NEW-EMPTY": "remote",
    "R-NEW-LOST-ACK": "remote",
    "R-PROJECT-UI": "remote",
    "R-TITLE-HTTP-MCP-AUTO": "remote",
    "R-TITLE-RETRY": "remote",
    "R-ARCHIVE-GUARDS": "remote",
    "R-HOST-ISOLATION": "remote",
    "R-NAV-DRAG": "remote",
}
MODES = {"legacy", "native5", "remote"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def read(path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def json_digest(value):
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True,
                         separators=(",", ":"), allow_nan=False).encode()
    return hashlib.sha256(payload).hexdigest()


def review_digest(manifest):
    return json_digest({key: value for key, value in manifest.items() if key != "review"})


def current_commit(root):
    return subprocess.check_output(
        ["git", "--no-optional-locks", "-C", str(root), "rev-parse", "HEAD"],
        text=True, timeout=10,
    ).strip()


def observation_commit(record):
    commit = record.get("observed_source_commit", record.get("source_commit"))
    require(isinstance(commit, str) and len(commit) in {40, 64}
            and all(character in "0123456789abcdef" for character in commit)
            and record.get("source_commit", commit) == commit, "Invalid observation commit")
    return commit


def is_ancestor(root, observed, final):
    process = subprocess.run(
        ["git", "--no-optional-locks", "-C", str(root),
         "merge-base", "--is-ancestor", observed, final], capture_output=True, timeout=10,
    )
    require(process.returncode in {0, 1}, "Cannot verify observation commit ancestry")
    return process.returncode == 0


def index(records, expected, label):
    require(isinstance(records, list), f"{label} must be a list")
    result = {}
    for record in records:
        identifier = record["id"]
        require(identifier not in result, f"Duplicate {label}: {identifier}")
        result[identifier] = record
    require(set(result) == set(expected), f"Missing or unexpected {label}")
    return result


def references(value):
    if isinstance(value, dict):
        if "path" in value and "sha256" in value:
            yield value
        for child in value.values():
            yield from references(child)
    elif isinstance(value, list):
        for child in value:
            yield from references(child)


def referenced_path(manifest_path, reference):
    require(isinstance(reference["path"], str) and reference["path"], "Empty artifact path")
    return (manifest_path.parent / reference["path"]).resolve()


def protect_report(root, manifest_path, manifest, report_path):
    contract = root / "acceptance/requirements.json"
    bindings = root / "acceptance/bindings.json"
    validate_report_path(root, contract, bindings, None, report_path)
    inputs = [manifest_path]
    inputs.extend(referenced_path(manifest_path, entry) for entry in references(manifest))
    require(not any(same_file(report_path, path) for path in inputs),
            "Report would overwrite manifest or referenced input")
    evidence = referenced_path(manifest_path, manifest["legacy_evidence"])
    validate_report_path(root, contract, bindings, evidence, report_path)


def evidence_result(identifier, record, manifest, path, contract_sha, build_sha):
    require(record.get("status") in {"pass", "fail", "pending"},
            f"Invalid evidence status: {identifier}")
    envelope = {**manifest, "checks": {identifier: record}}
    status, detail = live_result(identifier, envelope, path, contract_sha, build_sha)
    return {"id": identifier, "status": status, "detail": detail}


def legacy_report(manifest, path, contract, bindings, contract_sha, build_sha, modes):
    report = read(referenced_path(path, manifest["legacy_report"]))
    require(report["schema_version"] == 1 and report["scope"] == "release"
            and all(report.get(key) is True for key in
                    ("passed", "release_ready", "automated_passed")),
            "Legacy report must pass the complete release scope")
    require(report["contract_id"] == contract["contract_id"]
            and report["contract_sha256"] == contract_sha
            and report["build_sha256"] == build_sha, "Stale legacy contract or source")
    requested = set(validate_contract(contract, bindings))
    tests = validate_results({"protocol": 1, "results": report["tests"]}, requested)
    require(report["test_count"] == len(requested)
            and all(record["status"] == "pass" for record in tests.values()),
            "Legacy tests are missing, skipped or failed")
    requirements = index(report["requirements"], bindings["requirements"], "legacy requirement")
    evidence_path = referenced_path(path, manifest["legacy_evidence"])
    evidence = read(evidence_path)
    require(evidence["mode"] == "legacy"
            and evidence["configId"] == modes["legacy"]["configId"]
            and evidence["binary_sha256"] == manifest["binary"]["sha256"]
            and observation_commit(evidence) == observation_commit(manifest),
            "Legacy evidence has a different mode/config/binary/source identity")
    results = []
    for requirement in contract["requirements"]:
        identifier = requirement["id"]
        row = requirements[identifier]
        require(row["status"] == "pass"
                and row["automated_tests"] == bindings["requirements"][identifier],
                f"Legacy requirement did not pass its current bindings: {identifier}")
        if requirement["verification"] in {"mixed", "live"}:
            require(row["live_status"] == "pass", f"Missing legacy live result: {identifier}")
            status, detail = live_result(identifier, evidence, evidence_path,
                                         contract_sha, build_sha)
            results.append({"id": identifier, "status": status, "detail": detail})
    return results


def mode_records(manifest, path, binary_sha):
    modes = index(manifest["modes"], MODES, "mode")
    identities = set()
    directories = set()
    for name, mode in modes.items():
        config_path = referenced_path(path, mode["config"])
        config = read(config_path)
        info = read(referenced_path(path, mode["info"]))
        require(config["legacy_mode"] is (name == "legacy")
                and config["native_sections"] is (name != "legacy")
                and config["host_id"] == "local" and config["proxy_command"] == [],
                f"Incorrect root-service mode: {name}")
        require(config["sync_sections"] is False, f"Retired section sync enabled: {name}")
        if name == "legacy":
            require(config["clock_command"] == [] and config["sidebar_socket"] == ""
                    and config["auto_archive_seconds"] is None, "Unsafe legacy mode")
        directory = Path(config["data_dir"])
        require(directory.is_absolute() and directory.resolve() == directory,
                "Use resolved effective configs")
        config_id = json_digest(config)[:16]
        require(mode["configId"] == config_id and info["configId"] == config_id,
                f"Config identity mismatch: {name}")
        require(info["app"] == "codex-session-board" and info["build"] == binary_sha
                and info["dataDir"] == config["data_dir"]
                and isinstance(info["version"], str) and info["version"]
                and type(info["pid"]) is int and info["pid"] > 0,
                f"Runtime info does not match final binary/config: {name}")
        preparation = manifest.get("version_preparation")
        if preparation is not None:
            require(preparation["base_commit"] == observation_commit(manifest)
                    and preparation["version"] == info["version"]
                    and isinstance(preparation["notes"], str) and preparation["notes"],
                    "Version preparation does not match the observed runtime")
        require(config_id not in identities and config["data_dir"] not in directories,
                "Modes must use separate configs and data directories")
        identities.add(config_id)
        directories.add(config["data_dir"])
    remote = modes["remote"]
    registry_path = referenced_path(path, remote["registry"])
    config = read(referenced_path(path, remote["config"]))
    require(same_file(registry_path, Path(config["sidebar_state_file"])),
            "Remote registry is not the configured snapshot")
    host = remote["hostId"]
    require(isinstance(host, str) and host and host != "local", "Missing remote target")
    registry = read(registry_path)
    hosts = registry["codex-managed-remote-connections"]
    require(sum(entry["hostId"] == host for entry in hosts) == 1
            and registry.get("remote-connection-auto-connect-by-host-id", {}).get(host) is not False,
            "Remote target is absent, duplicated or disabled")
    return modes


def audit_records(manifest, modes):
    audit = manifest["audit"]
    allowed = set()
    for entry in audit["allowed_sessions"]:
        host, thread = entry["hostId"], entry["threadId"]
        require(isinstance(host, str) and host and str(uuid.UUID(thread)) == thread,
                "Invalid allowlist identity")
        require((host, thread) not in allowed, "Duplicate allowlist identity")
        allowed.add((host, thread))
    require(any(host == "local" for host, _ in allowed)
            and any(host == modes["remote"]["hostId"] for host, _ in allowed),
            "Allowlist must cover local and target remote test sessions")
    count = audit["outside_allowlist_forwarded_writes"]
    require(type(count) is int and count == 0, "Writes escaped the test allowlist")
    for write in audit["forwarded_writes"]:
        require((write["hostId"], write["threadId"]) in allowed
                and isinstance(write["method"], str) and write["method"],
                "Forwarded write is outside the allowlist")
    require(manifest["cleanup"]["remaining_owned_resources"] == [], "Cleanup is incomplete")


def verify(root, manifest_path, binary):
    manifest = read(manifest_path)
    contract_path = root / "acceptance/requirements.json"
    contract = read(contract_path)
    bindings = read(root / "acceptance/bindings.json")
    validate_contract(contract, bindings)
    require(len(contract["requirements"]) == 31, "Expected the frozen 31 requirements")
    lock = read(contract_path.with_name("contract.lock.json"))
    require({"requirements.json", "model.py"} <= set(lock["files"]), "Incomplete contract lock")
    for name, expected in lock["files"].items():
        require(digest(inside(contract_path.parent, contract_path.parent / name)) == expected,
                f"Changed contract baseline: {name}")
    contract_sha = digest(contract_path)
    build_sha = fingerprint(root, bindings["fingerprint_paths"])
    source = current_commit(root)
    observed_commit = observation_commit(manifest)
    require(is_ancestor(root, observed_commit, source), "Observation commit is not an ancestor of HEAD")
    binary_sha = digest(binary)
    require(manifest["schema_version"] == 1
            and manifest["contract_sha256"] == contract_sha
            and manifest["build_sha256"] == build_sha, "Stale manifest contract/source")
    require(digest(referenced_path(manifest_path, manifest["binary"])) == binary_sha
            and manifest["binary"]["sha256"] == binary_sha, "Final binary mismatch")
    inputs = {**manifest, "artifacts": list(references(manifest))}
    results = [evidence_result("inputs", inputs, manifest, manifest_path, contract_sha, build_sha)]
    modes = mode_records(manifest, manifest_path, binary_sha)
    results.extend(legacy_report(manifest, manifest_path, contract, bindings,
                                 contract_sha, build_sha, modes))
    for name, mode in modes.items():
        record = {**mode, "artifacts": [mode["config"], mode["info"]]}
        results.append(evidence_result(name, record, manifest, manifest_path,
                                       contract_sha, build_sha))
    checks = index(manifest["checks"], CASES, "extension case")
    for identifier, mode in CASES.items():
        record = checks[identifier]
        require(record["mode"] == mode and record["configId"] == modes[mode]["configId"]
                and record["binary_sha256"] == binary_sha and record["build_sha256"] == build_sha,
                f"Case mode/config/binary/source mismatch: {identifier}")
        results.append(evidence_result(identifier, record, manifest, manifest_path,
                                       contract_sha, build_sha))
    audit_records(manifest, modes)
    for identifier in ("audit", "cleanup", "review"):
        results.append(evidence_result(identifier, manifest[identifier], manifest,
                                       manifest_path, contract_sha, build_sha))
    require(manifest["review"]["subject_sha256"] == review_digest(manifest),
            "Review does not cover this manifest")
    observed = [manifest, *modes.values(), *checks.values(), manifest["audit"], manifest["cleanup"]]
    require(datetime.fromisoformat(manifest["cleanup"]["observed_at"]) >=
            max(datetime.fromisoformat(record["observed_at"]) for record in
                [*modes.values(), *checks.values()]), "Cleanup predates the test observations")
    require(datetime.fromisoformat(manifest["review"]["observed_at"]) >=
            max(datetime.fromisoformat(record["observed_at"]) for record in observed),
            "Review predates its evidence")
    require(current_commit(root) == source
            and fingerprint(root, bindings["fingerprint_paths"]) == build_sha
            and digest(binary) == binary_sha, "Inputs changed during verification")
    passed = all(result["status"] == "pass" for result in results)
    status = "pass" if passed else "fail" if any(
        result["status"] == "fail" for result in results) else "pending"
    return {
        "schema_version": 1, "scope": "remote-release", "status": status,
        "passed": passed, "release_ready": passed, "observed_source_commit": observed_commit,
        "final_source_commit": source, "attestation_commit": source,
        "version_preparation": manifest.get("version_preparation"), "binary_path": str(binary),
        "contract_sha256": contract_sha, "build_sha256": build_sha,
        "binary_sha256": binary_sha, "modes": modes, "results": results,
        "content_review_boundary": "Checks evidence identity and integrity, not GUI semantics or truth.",
    }, 0 if passed else 1 if status == "fail" else 2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    safe_report = False
    try:
        if args.report:
            protect_report(args.root.resolve(), args.manifest.resolve(),
                           read(args.manifest), args.report)
            require(not same_file(args.report, args.binary), "Report would overwrite executable")
        safe_report = True
        report, code = verify(args.root.resolve(), args.manifest.resolve(), args.binary.resolve())
    except (OSError, ValueError, KeyError, TypeError, AttributeError,
            subprocess.SubprocessError) as error:
        report = {"schema_version": 1, "scope": "remote-release", "status": "fail",
                  "passed": False, "release_ready": False, "error": str(error)}
        code = 1
    if args.report and safe_report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
