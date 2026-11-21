import json
from pathlib import Path
import subprocess
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
            self.assertEqual(started["dispatch"], {"action": "create"})
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
        args.head, args.command = REPAIR, "reserve-review"
        with patch.object(discovery.forge, "api", return_value=pull(31, head=REPAIR)):
            reservation = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual(reservation["dispatch"], {"action": "resume", "task": "reviewer"})
            self.assertNotEqual(reservation["request_id"], first["request_id"])
            args.command = "begin-review"
            started = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual(started["dispatch"], reservation["dispatch"])
        args.command = "record-review"
        with self.assertRaisesRegex(ValueError, "dedicated reviewer"):
            discovery.review_command(self.directory, path, data, args, "coordinator")
        args.review_task = "reviewer"
        discovery.review_command(self.directory, path, data, args, "coordinator")
        save(discovery.review_path(self.directory, 31, REPAIR),
             {"head": HEAD, "passed": True, "reviewer": "reviewer"})
        self.assertEqual(discovery.review_state(self.directory, claim, "author")["result"], "waiting-review")
        save(discovery.review_path(self.directory, 31, REPAIR),
             {"head": REPAIR, "passed": True, "reviewer": "reviewer"})
        self.assertEqual(discovery.review_state(self.directory, claim, "author")["result"], "review-passed")
        self.assertEqual(discovery.refresh_claim(claim, pull(31, branch="other"))["result"], "needs-user")
        self.assertEqual(claim["source_branch"], "codex/author")

    def test_legacy_review_route_uses_latest_recorded_session_and_waits_for_unknown_dispatch(self):
        path, data = discovery.load_claims(self.directory, REPOSITORY)
        claim = discovery.claim_one(path, data, [discovery.describe(pull(31))])
        intermediate = "d" * 40
        claim["author"] = {"thread_id": "author"}
        claim["heads"][HEAD]["review"] = {"dispatch_started_at": "first-start"}
        discovery.refresh_claim(claim, pull(31, head=intermediate))
        claim["heads"][intermediate]["review"] = {"dispatch_started_at": "previous-start"}
        discovery.refresh_claim(claim, pull(31, head=REPAIR))
        args = SimpleNamespace(command="reserve-review", pr=31, head=REPAIR)
        with patch.object(discovery.forge, "api", return_value=pull(31, head=REPAIR)):
            waiting = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual(waiting["result"], "waiting-review")
            self.assertEqual(waiting["dispatch"], {"action": "recover", "head": intermediate})
            self.assertNotIn("review", claim["heads"][REPAIR])
            prior = SimpleNamespace(command="record-review", pr=31, head=intermediate, review_task="latest")
            discovery.review_command(self.directory, path, data, prior, "coordinator")
            waiting = discovery.review_command(self.directory, path, data, args, "coordinator")
            self.assertEqual(waiting["dispatch"], {"action": "recover", "head": HEAD})
            prior.head, prior.review_task = HEAD, "first"
            discovery.review_command(self.directory, path, data, prior, "coordinator")
            save(discovery.review_path(self.directory, 31, REPAIR),
                 {"head": REPAIR, "passed": True, "reviewer": "first"})
            self.assertEqual(discovery.review_state(self.directory, claim, "author")["result"], "review-required")
            reserved = discovery.review_command(self.directory, path, data, args, "coordinator")
        self.assertEqual(reserved["dispatch"], {"action": "resume", "task": "latest"})
        another = discovery.claim_one(path, data, [discovery.describe(pull(32))])
        self.assertEqual(discovery.review_route(another, "coordinator"), {"action": "create"})

    def test_first_claim_rechecks_the_label_before_it_writes(self):
        def api(endpoint):
            return {"object": {"sha": MAIN}} if endpoint.endswith("/heads/main") else pull(31, labels=[])

        with patch.object(discovery.forge, "pages", return_value=[pull(31)]), \
                patch.object(discovery.forge, "api", side_effect=api):
            result = duty.run(None, self.config, self.directory, self.directory, SimpleNamespace(command="tick"))
        self.assertEqual(result["result"], "infrastructure-blocked")
        self.assertFalse((self.directory / "claims.json").exists())

    def test_upgrade_preserves_imported_reviewer_before_replacing_legacy_result(self):
        for source in ("result", "receipt", "newer-dispatch"):
            with self.subTest(source=source):
                row = pull(31, head=REPAIR)
                claim = {**discovery.describe(row), "repository": REPOSITORY, "heads": {HEAD: {}, REPAIR: {}},
                         "author": {"thread_id": "author"}}
                receipt = {"head": HEAD, "passed": True, "reviewer": "independent"}
                path = discovery.review_path(self.directory, 31, HEAD)
                expected = "independent"
                if source == "result":
                    claim["last_result"] = {"review": {"result": "review-passed", "reviewer": "independent",
                                                      "review": str(path)}}
                else:
                    save(path, receipt)
                if source == "newer-dispatch":
                    expected = "recent-reviewer"
                    claim["heads"][REPAIR]["review"] = {"task": expected}
                discovery.remember_legacy_review(self.directory, claim, "coordinator")
                claim["last_result"] = {"result": "review-required"}
                self.assertEqual(discovery.review_route(claim, "coordinator"),
                                 {"action": "resume", "task": expected})

    def test_completed_pr_remains_visible_while_the_existing_receipt_is_unconfirmed(self):
        for state in ("sent", "delivery_unknown", "rejected"):
            entry = {"status": "merged", "pr": 31, "last_result": {"delivery": {"author": {"state": state}}}}
            self.assertEqual(duty.worklist({"claims": {"31": entry}}, {}), [entry])
        entry["last_result"]["delivery"]["author"]["state"] = "delivered"
        self.assertEqual(duty.worklist({"claims": {"31": entry}}, {}), [])

    def test_author_binding_only_accepts_route_updates_for_the_original_session(self):
        claim = {}
        original = {"route": {"thread_id": "author", "socket": "/original"}}
        refreshed = {"route": {"thread_id": "author", "socket": "/replacement"}}
        duty.bind_author(claim, original)
        duty.bind_author(claim, refreshed)
        with self.assertRaisesRegex(ValueError, "Original author session changed"):
            duty.bind_author(claim, {"route": {"thread_id": "another-author"}})
        self.assertEqual(claim["author"], refreshed["route"])

    def test_frozen_claim_scope_rejects_same_commit_on_a_different_branch(self):
        scope = {"source_repository": REPOSITORY, "source_branch": "codex/author", "target_branch": "main"}
        with patch.object(discovery.forge, "api", return_value=pull(31, branch="codex/other")):
            with self.assertRaisesRegex(duty.gates.GateBlocked, "originally claimed"):
                duty.gates.verify({"repository": REPOSITORY, "items": [{"number": 31, "head": HEAD, "scope": scope}]})

    def test_manual_ci_failure_reuses_same_feedback_and_automatic_task_reuses_event(self):
        claim = {"repository": REPOSITORY, "pr": 31, "head": HEAD}
        ci = {"run": 55, "attempt": 2, "conclusion": "failure", "url": "https://example.test/run",
              "jobs": [{"name": "changes", "url": "https://example.test/job"}]}
        report = {"workflow": ci}
        record = {}
        feedback = {"result": "needs-author", "feedback": {"id": "failure"}}
        with patch.object(duty.authors, "post_feedback", return_value=feedback) as post:
            self.assertEqual(duty.authors.ci_feedback(claim, None, report, record)["feedback"]["id"], "failure")
            self.assertEqual(duty.authors.ci_feedback(claim, None, report, record)["feedback"]["id"], "failure")
            task = {"author_task": "/task", "mode": "automatic", "events": [{
                "head": HEAD, "items": [{"kind": "check", "url": ci["jobs"][0]["url"]}],
                "delivery": {"state": "sent", "turn_id": "original"},
            }]}
            reused = duty.authors.ci_feedback(claim, task, report, record)
            self.assertEqual(reused["delivery"]["author"]["turn_id"], "original")
            post.assert_called_once()

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

    def test_recovery_reuses_only_the_expected_mechanical_merge(self):
        for stage in ("staged", "committed", "modified"):
            with self.subTest(stage=stage):
                queue = self.directory / stage
                queue.mkdir()

                def git(*arguments):
                    return subprocess.run(["git", "-C", str(queue), *arguments], check=True,
                                          capture_output=True, text=True).stdout.strip()

                git("init", "-b", "codex/candidate")
                git("config", "user.name", "Fixture")
                git("config", "user.email", "fixture@example.test")
                git("config", "commit.gpgSign", "false")
                git("commit", "--allow-empty", "-m", "main")
                prefix = git("rev-parse", "HEAD")
                git("switch", "-c", "author")
                (queue / "feature").write_text("author version")
                git("add", "feature")
                git("commit", "-m", "feature")
                head = git("rev-parse", "HEAD")
                state = {"prefix": prefix, "events": [],
                         "items": [{"number": 31, "head": head, "status": "submitted"}]}
                state_dir = queue / ".git" / "queue-state"
                state_dir.mkdir()
                save(state_dir / "queue.json", state)
                git("switch", "codex/candidate")
                git("merge", "--no-ff", "--no-commit", head)
                if stage == "modified":
                    (queue / "feature").write_text("changed after the merge")
                    git("add", "feature")
                    with self.assertRaisesRegex(ValueError, "Interrupted index"):
                        queues.recover_combination(state_dir, state, queue)
                    self.assertEqual(json.loads((state_dir / "queue.json").read_text())["prefix"], prefix)
                    continue
                if stage == "committed":
                    git("commit", "-m", "merge: queue PR 31")
                with patch.object(queues.pr_queue.shutil, "which", return_value=None):
                    queues.recover_combination(state_dir, state, queue)
                recovered = json.loads((state_dir / "queue.json").read_text())
                self.assertEqual(recovered["prefix"], git("rev-parse", "HEAD"))
                self.assertEqual(git("rev-list", "--parents", "-n", "1", recovered["prefix"]).split()[1:],
                                 [prefix, head])
                self.assertEqual(recovered["items"][0]["status"], "accepted")
                self.assertTrue(recovered["events"][0]["mechanical_recovery"])

    def test_follow_retains_exact_ci_run_without_using_stale_author_evidence(self):
        first = self.round([pull(31)])
        path, data = discovery.load_claims(self.directory, REPOSITORY)
        claim = data["claims"]["31"]
        self.assertEqual(first["claimed"]["head"], HEAD)
        save(discovery.review_path(self.directory, 31, REPAIR),
             {"head": REPAIR, "passed": True, "reviewer": "independent"})
        author = {"route": {"thread_id": "author"}, "delivery_head": HEAD}
        run = {"run": 107, "attempt": 1, "head": REPAIR, "status": "in_progress"}
        registry_project = self.directory / "registry"
        self.config["registry_project"] = str(registry_project)
        with patch.object(discovery.forge, "api", return_value=pull(31, head=REPAIR)), \
                patch.object(queues, "locate", return_value=None) as locate, \
                patch.object(duty.authors, "delivery", return_value=author) as delivery, \
                patch.object(duty.gates, "workflow", return_value=run):
            result = duty.follow(None, self.config, self.directory, self.directory, path, data, claim, MAIN)
        locate.assert_called_once_with(registry_project, claim, self.directory)
        delivery.assert_called_once_with(self.config, registry_project, claim, None)
        self.assertEqual(result["result"], "waiting-ci")
        self.assertFalse(result["author_evidence"])
        _, loaded = discovery.load_claims(self.directory, REPOSITORY)
        self.assertEqual(loaded["claims"]["31"]["heads"][REPAIR]["ci_runs"]["107"], run)
        self.assertEqual(loaded["claims"]["31"]["review_task"], "independent")

    def test_recovered_queue_stays_registered_when_the_pr_closed_between_rounds(self):
        self.round([pull(31)])
        path, data = discovery.load_claims(self.directory, REPOSITORY)
        claim = data["claims"]["31"]
        registry_project = self.directory / "registry"
        self.config["registry_project"] = str(registry_project)
        queue = self.directory / "queue-pr-31"
        queue.mkdir()
        saved_queue = {"repository": REPOSITORY, "feedback_mode": "github",
                       "items": [{"number": 31, "status": "submitted"}]}
        save(queue / "queue.json", saved_queue)
        remote = {**pull(31), "state": "closed", "merged": False, "merge_commit_sha": None}
        index = registry_project / "index"
        with patch.object(discovery.forge, "api", return_value=remote), \
                patch.object(duty.registry, "registry", return_value=index):
            result = duty.follow(None, self.config, self.directory, self.directory, path, data, claim, MAIN)
            self.assertEqual(duty.registry.locate(REPOSITORY, 31, registry_project), queue)
        self.assertEqual(result["result"], "closed-without-merge")
