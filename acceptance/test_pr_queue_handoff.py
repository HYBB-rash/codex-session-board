from argparse import Namespace
import copy
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import BASE, HEAD, pull
from scripts import pr_queue as queue
from scripts import pr_queue_cli as cli
from scripts import pr_queue_handoff as handoff
from scripts import pr_queue_registry as registry


CANDIDATE = "c" * 40


class PRQueueHandoffTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.state = {
            "repository": "owner/repo", "feedback_mode": "github", "project": str(self.directory),
            "checker": {"commit": "checker"}, "base": BASE, "prefix": CANDIDATE, "events": [],
            "started_at": queue.now(), "items": [{
                "number": 7, "head": HEAD, "base": BASE, "status": "accepted",
                "review": "review.json", "validation": "validation", "route": {"thread_id": "author"},
            }], "frozen_candidate": {"candidate": CANDIDATE, "status": "pending"},
        }
        self.pr = pull()
        self.main = BASE
        self.snapshot = {"head": HEAD, "state": "open", "ci": "passed", "items": []}
        self.arguments = Namespace(command="resume", item=None)
        for target, options in (
            (registry, {"registry": lambda: self.directory / "registry"}),
            (handoff.forge, {"api": self.api, "snapshot": lambda state: copy.deepcopy(self.snapshot)}),
            (queue, {"verify_checker": lambda state: None, "verify_entry": lambda state, item: None,
                     "clean": lambda project, source: None, "git": lambda *args: CANDIDATE}),
        ):
