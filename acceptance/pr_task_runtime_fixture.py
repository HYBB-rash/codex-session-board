import json
import os
from pathlib import Path
import subprocess
import sys

from acceptance.pr_task_fixtures import pull, task
from scripts.pr_task_state import ROOT, persist
from scripts.validate_candidate import identity


def runtime(test):
    state, _ = task(test)
    directory = Path(state["directory"])
    project = directory / "project"
    project.mkdir()
    def git(*arguments):
        return subprocess.check_output(["git", *arguments], cwd=project, text=True, stderr=subprocess.PIPE).strip()
    git("init", "-b", "codex/task")
    git("config", "user.name", "PR Task Fixture")
    git("config", "user.email", "fixture@example.invalid")
    git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", "fixture")
    git("remote", "add", "origin", "https://github.com/owner/repo")
    head = git("rev-parse", "HEAD")
    state.update(project=str(project), base_commit=head, checker=identity(ROOT), checker_project=str(ROOT))
    state["route"].update(expected_cwd=str(project), repair_cwd=str(project))
    persist(state)
    pr = pull()
    pr["head"]["sha"] = head
    pr["base"]["sha"] = head
    database = directory / "forge.json"
    database.write_text(json.dumps({
        "pr": pr, "comments": [], "labels": [], "calls": [],
        "checks": [{"id": 1, "head_sha": head, "name": "changes", "status": "completed",
                    "conclusion": "success", "html_url": "https://github.com/fixture/check"}],
    }))
    adapter = directory / "fixture-gh"
    adapter.write_text(
        f"#!{sys.executable}\nimport sys\nsys.path.insert(0, {str(ROOT)!r})\n"
        f"from acceptance.pr_task_forge_fixture import main\nmain({str(database)!r})\n")
    adapter.chmod(0o700)
    environment = dict(os.environ, CODEX_GITHUB_COMMAND=json.dumps([str(adapter)]),
                       CODEX_THREAD_ID=state["route"]["thread_id"])
    return state, database, environment


def command(state, environment, action, *arguments, check=True):
    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts/pr_task.py"), action, "--state", state["directory"], *arguments],
        env=environment, cwd=ROOT, text=True, capture_output=True, timeout=150)
    if check and result.returncode:
        raise RuntimeError(f"{action} exited {result.returncode}: {result.stderr}")
    return json.loads(result.stdout) if check else result
