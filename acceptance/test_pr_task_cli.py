import json
from pathlib import Path
import unittest

from acceptance.pr_task_runtime_fixture import command, runtime
from acceptance.pr_task_fixtures import feedback, saved
from scripts import pr_task_lifecycle as lifecycle
from scripts.pr_task_state import locked


class PRTaskCLITests(unittest.TestCase):
    def test_real_cli_idle_polling_uses_no_model_and_closes_without_deleting_work(self):
        state, database, environment = runtime(self)
        self.assertEqual(command(state, environment, "tick", "--force")["phase"], "watching")
        self.assertEqual(command(state, environment, "tick", "--force")["phase"], "watching")
        self.assertEqual(saved(state)["events"], [])
        data = json.loads(database.read_text())
        data["pr"].update(state="closed", merged=True, merge_commit_sha="d" * 40)
        database.write_text(json.dumps(data))
        result = command(state, environment, "tick", "--force")
        self.assertEqual(result["phase"], "complete")
        self.assertEqual(result["outcome"]["result"], "merged")
        self.assertTrue((Path(state["project"]) / ".git").exists())
        self.assertEqual(saved(state)["archive_receipt"]["status"], "not_requested")

    def test_real_cli_requires_original_claim_and_publishes_blocked_once(self):
        state, database, environment = runtime(self)
        data = json.loads(database.read_text())
        snapshot = {"head": data["pr"]["head"]["sha"]}
        event = lifecycle.create_event(state, snapshot, [feedback()])
        wrong = {**environment, "CODEX_THREAD_ID": "another"}
        self.assertEqual(command(state, wrong, "claim", "--event", event["id"], check=False).returncode, 1)
        self.assertEqual(command(state, environment, "claim", "--event", event["id"])["status"], "claimed")
        message = Path(state["directory"]) / "blocked.md"
        message.write_text("The requested behavior needs a decision.")
        result = command(state, environment, "complete", "--event", event["id"],
                         "--result", "blocked", "--message-file", str(message))
        self.assertEqual(result["phase"], "needs-human")
        command(state, environment, "tick", "--force")
        self.assertEqual(len(json.loads(database.read_text())["comments"]), 1)
        self.assertEqual(saved(state)["completed"], [])

    def test_real_cli_lock_and_background_permission_probe_are_idempotent(self):
        state, database, environment = runtime(self)
        with locked(Path(state["directory"])):
            result = command(state, environment, "tick", check=False)
            self.assertEqual(result.returncode, 75)
        first = command(state, environment, "probe")
        second = command(state, environment, "probe")
        self.assertEqual(first, second)
        data = json.loads(database.read_text())
        self.assertEqual(len(data["comments"]), 1)
        self.assertEqual({entry["name"] for entry in data["labels"]}, {"needs-fix", "ready-for-review"})
