import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import pr_task_github as forge
from scripts.pr_task_state import digest, git, load, locked, now, private_directory, save
from scripts.validate_candidate import identity


SHA = re.compile(r"[0-9a-f]{40}")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
REMOTE = re.compile(r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)(.+?)(?:\.git)?")
PRODUCTION = "hybb-rash/codex-session-board"


def read(path):
    return json.loads(Path(path).read_text())


def verify_origin(project, repository):
    for options in (("--all",), ("--push", "--all")):
        url = git(project, "remote", "get-url", *options, "origin")
        match = REMOTE.fullmatch(url)
        if "\n" in url or not match or match[1].lower() != repository.lower():
            raise ValueError("Candidate fetch and push origin must identify the configured repository")


def configuration(path, command="scan"):
    config = read(path)
    repository = config["repository"]
    if not REPOSITORY.fullmatch(repository):
        raise ValueError("Configure an explicit GitHub repository")
    mode = config.get("mode")
    production = repository.lower() == PRODUCTION
    if production and mode == "production-scan":
        if command != "scan":
            raise ValueError("Production duty only supports read-only scan")
        handoff = Path(config["handoff"])
        if not handoff.is_absolute() or "assignments" in config:
