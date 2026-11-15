import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts import pr_plan, rust_adapter
from scripts.validate_candidate import identity
from scripts.verify_acceptance import digest, read_json, validate_contract


def verify(strategy, destination):
    root = Path.cwd().resolve()
    started = time.monotonic()
    source = identity(root)
    record = {"scope": "pr_tests", "source": source, "plan": strategy, "passed": False,
              "release_ready": False, "started_at": datetime.now(timezone.utc).isoformat()}
    try:
        bindings = read_json(root / "acceptance/bindings.json")
        requested = validate_contract(read_json(root / "acceptance/requirements.json"), bindings)
        lock = read_json(root / "acceptance/contract.lock.json")
        for path in ("requirements.json", "model.py"):
            if digest(root / "acceptance" / path) != lock["files"][path]:
                raise ValueError("The accepted contract baseline changed")
        outcome = rust_adapter.run(
            {"protocol": 1, "tests": requested},
            prefixes=pr_plan.groups(requested, strategy["profile"]), include_python=True,
        )
        record.update(outcome)
        names = [item["id"] for item in outcome["results"]]
        if (not names or len(names) != len(set(names))
                or set(names) != set(outcome["selected"])
                or any(item["status"] != "pass" for item in outcome["results"])):
            raise ValueError("Selected PR tests failed, were skipped or have missing results")
        if identity(root) != source:
            raise ValueError("PR source changed while running tests")
        record["passed"] = True
        return 0
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        record["error"] = str(error)
        return 1
    finally:
        record.update(seconds=time.monotonic() - started,
                      finished_at=datetime.now(timezone.utc).isoformat())
        destination.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    arguments = parser.parse_args()
    destination = arguments.report.resolve()
    if destination.is_relative_to(Path.cwd().resolve()) or destination.exists():
        raise ValueError("PR test report must use a new path outside the source")
    destination.parent.mkdir(parents=True, exist_ok=True)
    return verify(read_json(arguments.plan), destination)


if __name__ == "__main__":
    sys.exit(main())
