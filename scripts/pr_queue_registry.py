import json
from pathlib import Path

from scripts.pr_task_state import ROOT, digest, git, locked, save


def registry(project=ROOT):
    return Path(git(project, "rev-parse", "--path-format=absolute", "--git-common-dir")) / "pr-queues"


def register(directory, state, *, project=None):
    index = registry() if project is None else registry(project)
    with locked(index) as folder:
        for item in state["items"]:
            path = folder / f"{digest([state['repository'].lower(), item['number']])}.json"
            record = json.loads(path.read_text()) if path.exists() else {"directories": []}
            entry = str(directory.resolve())
            if entry not in record["directories"]:
                record["directories"] = sorted(set(record["directories"]) | {entry})
                save(path, record)


def locate(repository, number, project=None):
    folder = registry() if project is None else registry(project)
    path = folder / f"{digest([repository.lower(), number])}.json"
    record = json.loads(path.read_text())
    matches = []
    for name in record["directories"]:
        directory = Path(name).resolve(strict=True)
        state = json.loads((directory / "queue.json").read_text())
        if state["repository"].lower() != repository.lower():
            raise ValueError("Registered queue changed repository")
        item = next((item for item in state["items"] if item["number"] == number), None)
        if item is None:
            raise ValueError("Registered queue no longer contains this PR")
        matches.append((directory, item))
    active = [entry for entry in matches if entry[1]["status"] not in {"merged", "closed"}]
    selected = active or matches
    if len(selected) != 1:
        raise ValueError("Multiple queue records match; inspect their identities and select --state")
    return selected[0][0]
