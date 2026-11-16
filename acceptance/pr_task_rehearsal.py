import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from acceptance.pr_task_runtime_fixture import command, runtime
from acceptance.pr_task_fixtures import saved
from scripts import pr_task_scheduler as scheduler
from scripts.pr_task_state import locked, persist


def main():
    arguments = argparse.ArgumentParser(description="Isolated real CLI/launchd rehearsal; no live GitHub or Codex calls")
    arguments.add_argument("--output", required=True, type=Path)
    options = arguments.parse_args()
    if sys.platform != "darwin":
        raise ValueError("This rehearsal exercises macOS launchd")
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    test = unittest.TestCase()
    state, database, environment = runtime(test)
    started = time.monotonic()
    try:
        idle = command(state, environment, "tick", "--force")
        assert idle["phase"] == "watching" and not saved(state)["events"]
        installed = command(state, environment, "schedule")
        label = installed["scheduler"]["label"]
        target = f"gui/{os.getuid()}/{label}"
        data = json.loads(database.read_text())
        data["pr"].update(state="closed", merged=True, merge_commit_sha="d" * 40)
        database.write_text(json.dumps(data))
        with locked(Path(state["directory"])):
            due = saved(state)
            due["next_poll_at"] = 0
            persist(due)
        subprocess.run(["/bin/launchctl", "kickstart", target], check=True, capture_output=True, timeout=20)
        deadline = time.monotonic() + 50
        while time.monotonic() < deadline and saved(state)["phase"] != "complete":
            time.sleep(0.2)
        final = saved(state)
        probe = subprocess.run(["/bin/launchctl", "print", target], capture_output=True, text=True, timeout=20)
        assert final["phase"] == "complete", final
        assert final["scheduler"]["status"] == "removed"
        assert not Path(final["scheduler"]["plist"]).exists()
        assert probe.returncode == 113, probe
        assert Path(final["project"]).is_dir()
        assert final["archive_receipt"]["status"] == "not_requested"
        result = {"passed": True, "seconds": round(time.monotonic() - started, 3),
                  "fixture_only": True, "real_launchd": True, "model_turns": len(final["events"]),
                  "final_state": final, "service_exit": probe.returncode}
        (output / "RESULT.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps({key: value for key, value in result.items() if key != "final_state"}))
    except Exception as error:
        (output / "RESULT.json").write_text(json.dumps({
            "passed": False, "seconds": round(time.monotonic() - started, 3),
            "error": str(error), "final_state": saved(state),
        }, ensure_ascii=False, indent=2) + "\n")
        raise
    finally:
        for name in ("scheduler.log", "scheduler-error.log", "finalizer.log", "events.jsonl", "forge.json"):
            source = Path(state["directory"]) / name
            if source.exists():
                (output / name).write_bytes(source.read_bytes())
        current = saved(state)
        if current["scheduler"] and current["scheduler"]["status"] != "removed":
            scheduler.remove(current)
        test.doCleanups()


if __name__ == "__main__":
    main()
