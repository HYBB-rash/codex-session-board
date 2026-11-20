import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from scripts import pr_merge_duty_discovery as discovery
from scripts import pr_merge_duty_production as duty
from scripts import pr_merge_duty_queue as queues
from scripts.pr_task_state import save


REPOSITORY = "Example/project"
MAIN = "a" * 40
HEAD = "b" * 40
REPAIR = "c" * 40


def pull(number, *, head=HEAD, draft=False, source=REPOSITORY, branch="codex/author", labels=None):
    return {
        "number": number, "html_url": f"https://github.com/{REPOSITORY}/pull/{number}",
        "head": {"repo": {"full_name": source}, "ref": branch, "sha": head},
        "base": {"repo": {"full_name": REPOSITORY}, "ref": "main"},
        "labels": [{"name": label} for label in (["ready-for-review"] if labels is None else labels)],
        "state": "open", "draft": draft,
    }


class PRDutyClaimTests(unittest.TestCase):
    def setUp(self):
        folder = TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.directory = Path(folder.name)
        self.config = {"repository": REPOSITORY, "checker_head": MAIN}

    def round(self, rows, mode="tick"):
        def api(endpoint):
            if endpoint.endswith("/heads/main"):
                return {"object": {"sha": MAIN}}
            return next(row for row in rows if str(row["number"]) == endpoint.rsplit("/", 1)[-1])

        with patch.object(discovery.forge, "pages", return_value=rows), \
                patch.object(discovery.forge, "api", side_effect=api), \
                patch.object(duty, "follow", return_value={"result": "waiting-ci", "workflow": {"run": 42}}):
            return duty.run(None, self.config, self.directory, self.directory, SimpleNamespace(command=mode))

    def test_full_discovery_claims_one_and_keeps_other_ready_prs_visible(self):
        rows = [pull(1, draft=True), pull(2, source="Fork/project"), pull(3, labels=[]), pull(31), pull(32)]
        read = self.round(rows, "scan")
        self.assertEqual([item["pr"] for item in read["pending"]], [31, 32])
        self.assertFalse((self.directory / "claims.json").exists())
        first = self.round(rows)
        self.assertEqual(first["claimed"]["pr"], 31)
        self.assertEqual([item["pr"] for item in first["pending"]], [32])
        self.assertEqual(first["item"]["workflow"]["run"], 42)
        self.assertNotIn("claimed", self.round(rows))
        self.assertEqual(self.round(rows)["claimed"]["pr"], 32)
        _, data = discovery.load_claims(self.directory, REPOSITORY)
        self.assertEqual(set(data["claims"]), {"31", "32"})

    def test_duplicate_or_failed_pagination_is_never_empty_or_a_partial_claim(self):
        duplicate = self.round([pull(31), pull(31)])
        self.assertEqual(duplicate["result"], "infrastructure-blocked")
        with patch.object(discovery.forge, "pages", side_effect=discovery.forge.ForgeError("page two failed")):
            broken = duty.run(None, self.config, self.directory, self.directory, SimpleNamespace(command="tick"))
        self.assertEqual(broken["result"], "infrastructure-blocked")
        self.assertEqual(json.loads((self.directory / "last-tick.json").read_text()), broken)
        self.assertFalse((self.directory / "claims.json").exists())
        self.assertEqual(self.round([])["result"], "idle")

    def test_review_reservation_is_idempotent_and_new_head_keeps_old_receipt(self):
        rows = [pull(31)]
        self.round(rows)
        path, data = discovery.load_claims(self.directory, REPOSITORY)
        claim = data["claims"]["31"]
        claim["author"] = {"thread_id": "author"}
        args = SimpleNamespace(command="reserve-review", pr=31, head=HEAD)
        with patch.object(discovery.forge, "api", return_value=rows[0]):
            first = discovery.review_command(self.directory, path, data, args, "coordinator")
            path, data = discovery.load_claims(self.directory, REPOSITORY)
            second = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual((first["result"], second["result"]), ("review-reserved", "review-reserved"))
            self.assertEqual(first["request"], second["request"])
            args.command = "begin-review"
            started = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual(started["result"], "dispatch-review")
            self.assertEqual(discovery.review_command(self.directory, path, data, args, "coordinator")["result"],
                             "waiting-review")
        args.command = "record-review"
        args.review_task = "reviewer"
        discovery.review_command(self.directory, path, data, args, "coordinator")
        args.review_task = "another-reviewer"
        with self.assertRaisesRegex(ValueError, "already recorded"):
            discovery.review_command(self.directory, path, data, args, "coordinator")
        claim = data["claims"]["31"]
        save(discovery.review_path(self.directory, 31, HEAD),
             {"head": HEAD, "passed": True, "reviewer": "reviewer"})
        self.assertEqual(discovery.review_state(self.directory, claim, "author")["result"], "review-passed")
        self.assertIsNone(discovery.refresh_claim(claim, pull(31, head=REPAIR)))
        self.assertEqual(discovery.review_state(self.directory, claim, "author")["result"], "review-required")
        self.assertEqual(claim["heads"][HEAD]["review"]["task"], "reviewer")
        self.assertEqual(discovery.refresh_claim(claim, pull(31, branch="other"))["result"], "needs-user")
        self.assertEqual(claim["source_branch"], "codex/author")

    def test_first_claim_rechecks_the_label_before_it_writes(self):
        def api(endpoint):
            return {"object": {"sha": MAIN}} if endpoint.endswith("/heads/main") else pull(31, labels=[])

        with patch.object(discovery.forge, "pages", return_value=[pull(31)]), \
                patch.object(discovery.forge, "api", side_effect=api):
            result = duty.run(None, self.config, self.directory, self.directory, SimpleNamespace(command="tick"))
        self.assertEqual(result["result"], "infrastructure-blocked")
        self.assertFalse((self.directory / "claims.json").exists())

    def test_completed_pr_remains_visible_while_the_existing_receipt_is_unconfirmed(self):
        for state in ("sent", "delivery_unknown", "rejected"):
            entry = {"status": "merged", "pr": 31, "last_result": {"delivery": {"author": {"state": state}}}}
            self.assertEqual(duty.worklist({"claims": {"31": entry}}, {}), [entry])
        entry["last_result"]["delivery"]["author"]["state"] = "delivered"
        self.assertEqual(duty.worklist({"claims": {"31": entry}}, {}), [])

    def test_recombination_reuses_queue_identity_and_preserves_old_candidate(self):
        queue = self.directory
        state = {"project": "/prior", "base": MAIN, "prefix": HEAD, "integration": {"run": 42},
                 "frozen_candidate": {"candidate": HEAD}, "items": [{"head": HEAD, "status": "accepted",
                 "ingress": HEAD, "attempts": ["prior failure"]}]}
        save(queue / "queue.json", state)
        queues.replace_base(queue, state, Path("/new"), REPAIR, {"head": HEAD, "review": "/receipt"})
        saved = json.loads((queue / "queue.json").read_text())
        self.assertEqual((saved["base"], saved["prefix"], saved["project"]), (REPAIR, REPAIR, "/new"))
        self.assertEqual(saved["recombinations"][0]["integration"]["run"], 42)
        self.assertNotIn("frozen_candidate", saved)
        self.assertEqual(saved["items"][0]["status"], "submitted")
        self.assertEqual(saved["items"][0]["attempts"], ["prior failure"])

    def test_follow_retains_exact_ci_run_without_using_stale_author_evidence(self):
        first = self.round([pull(31)])
        path, data = discovery.load_claims(self.directory, REPOSITORY)
        claim = data["claims"]["31"]
        self.assertEqual(first["claimed"]["head"], HEAD)
        save(discovery.review_path(self.directory, 31, REPAIR),
             {"head": REPAIR, "passed": True, "reviewer": "independent"})
        author = {"route": {"thread_id": "author"}, "delivery_head": HEAD}
        run = {"run": 107, "attempt": 1, "head": REPAIR, "status": "in_progress"}
        with patch.object(discovery.forge, "api", return_value=pull(31, head=REPAIR)), \
                patch.object(queues, "locate", return_value=None), \
                patch.object(duty.authors, "delivery", return_value=author), \
                patch.object(duty.gates, "workflow", return_value=run):
            result = duty.follow(None, self.config, self.directory, self.directory, path, data, claim, MAIN)
        self.assertEqual(result["result"], "waiting-ci")
        self.assertFalse(result["author_evidence"])
        _, loaded = discovery.load_claims(self.directory, REPOSITORY)
        self.assertEqual(loaded["claims"]["31"]["heads"][REPAIR]["ci_runs"]["107"], run)
