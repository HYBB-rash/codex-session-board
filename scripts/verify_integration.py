import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import integration_plan, rust_adapter
from scripts.validate_candidate import identity
from scripts.verify_acceptance import digest, read_json, validate_contract


def verify(base):
    source = identity()
    expected_plan = integration_plan.plan(base)
    if expected_plan["profile"] == "full":
        raise ValueError("This candidate requires the complete automated acceptance entry")
    bindings = read_json(ROOT / "acceptance/bindings.json")
    requirements = read_json(ROOT / "acceptance/requirements.json")
    requested = validate_contract(requirements, bindings)
    lock = read_json(ROOT / "acceptance/contract.lock.json")
    for path in ("requirements.json", "model.py"):
        if digest(ROOT / "acceptance" / path) != lock["files"][path]:
            raise ValueError("The accepted contract baseline changed")
    chosen = integration_plan.selected(set(requested), expected_plan["profile"])
    groups = sorted({"::".join(name.split("::")[:2]) for name in chosen})
    outcome = rust_adapter.run({"protocol": 1, "tests": requested}, prefixes=groups)
    report = {
        "scope": "integration", "source": source, "plan": expected_plan,
        "passed": all(item["status"] == "pass" for item in outcome["results"]),
        "release_ready": False, "tests": outcome["results"],
        "inventory": outcome["inventory"], "selected": outcome["selected"],
        "not_run": outcome["not_run"], "timings": outcome["timings"],
    }
    if identity() != source:
        raise ValueError("Source changed during integration verification")
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--report", required=True, type=Path)
    arguments = parser.parse_args()
    destination = arguments.report.resolve()
    if destination.is_relative_to(ROOT) or destination.exists():
        raise ValueError("Integration report must use a new path outside the source")
    report = verify(arguments.base)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    integration_plan.validate_report(
        report, report["plan"], report["source"], set(report["inventory"]))
    print(json.dumps({"profile": report["plan"]["profile"],
                      "executed": len(report["tests"]), "not_run": len(report["not_run"]),
                      "passed": report["passed"], "timings": report["timings"]}))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
