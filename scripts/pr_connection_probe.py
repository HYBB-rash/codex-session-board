from contextlib import closing
from datetime import datetime, timezone
from http.client import HTTPConnection
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time

from package_scenarios import foreign_peer


def request(port, path, body=None, token=None):
    headers = {"Origin": f"http://127.0.0.1:{port}", "Content-Type": "application/json"}
    if token:
        headers["X-CSRF-Token"] = token
    with closing(HTTPConnection("127.0.0.1", port, timeout=4)) as connection:
        connection.request("GET" if body is None else "POST", path,
                           None if body is None else json.dumps(body), headers)
        with connection.getresponse() as response:
            return response.status, json.load(response)


def verify_sidebar(startup, package, fixture, output):
    started = time.monotonic()
    record = {"name": "wrong-sidebar", "passed": False, "cleanup_confirmed": False,
              "started_at": datetime.now(timezone.utc).isoformat()}
    try:
        with tempfile.TemporaryDirectory(prefix="csb-pr-sidebar-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            environment = startup.child_environment(root)
            with socket.socket() as lease:
                lease.bind(("127.0.0.1", 0))
                port = lease.getsockname()[1]
            subprocess.run([str(fixture), "prepare", str(root), str(port)],
                           cwd=startup.ROOT, env=environment, check=True, timeout=10,
                           capture_output=True)
            with foreign_peer(root) as (path, events):
                settings = json.loads((root / "config.json").read_text())
                settings["sidebar_socket"] = str(path)
                (root / "config.json").write_text(json.dumps(settings))
                children = []
                try:
                    children.append(startup.Child(root, "fixture", [
                        str(fixture), "serve", str(root), str(package / "bin/run-plugin")], environment))
                    deadline = time.monotonic() + 20
                    while True:
                        try:
                            _, snapshot = request(port, "/api/sessions")
                            eligible = [row for row in snapshot["sessions"]
                                        if isinstance(row.get("sectionVersion"), str)]
                            if snapshot["health"]["nativeVerified"] and eligible:
                                break
                        except (OSError, ValueError, KeyError):
                            pass
                        if time.monotonic() >= deadline or children[0].poll() is not None:
                            raise RuntimeError("The owned backend did not become ready for the sidebar probe")
                        time.sleep(0.1)
                    _, info = request(port, "/api/info")
                    if info["authority"]["pid"] != children[0].pid:
                        raise ValueError("Sidebar probe did not use its own fixture backend")
                    before = eligible[0]
                    body = {key: before[key] for key in ("id", "version", "inputVersion", "sectionVersion")}
                    body.update(state="blocked", note="隔离归属检查")
                    status, response = request(port, "/api/state", body, info["requestToken"])
                    _, after = request(port, "/api/sessions")
                    deadline = time.monotonic() + 1
                    while not events and time.monotonic() < deadline:
                        time.sleep(0.01)
                    record.update(status=status, response=response, bytes_per_connection=list(events),
                                  backend_pid=info["authority"]["pid"])
                    if not events or any(events):
                        raise ValueError(f"Foreign sidebar received bytes or was not contacted: {events}")
                    if status < 400 or "侧栏不属于" not in json.dumps(response, ensure_ascii=False):
                        raise ValueError("State write did not reject the foreign sidebar at the ownership boundary")
                    observed = next(row for row in after["sessions"] if row["id"] == before["id"])
                    keys = ("state", "note", "version", "inputVersion", "sectionVersion")
                    if any(observed[key] != before[key] for key in keys):
                        raise ValueError("Rejected sidebar operation changed the session")
                finally:
                    startup.reap(children)
                    record["cleanup_confirmed"] = True
            record["passed"] = True
            return record
    finally:
        record.update(seconds=time.monotonic() - started,
                      finished_at=datetime.now(timezone.utc).isoformat())
        (output / "wrong-sidebar.json").write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
