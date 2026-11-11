import argparse
import fcntl
import json
import os
import platform
import shutil
import shlex
import sqlite3
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

from package_plugin import TARGETS, command, verify_package
from reload_plugins import reload_plugins


MARKET = "session-board-rust"

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, new_url):
        raise ValueError("Board identity checks cannot follow redirects")


def target():
    targets = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
               ("Darwin", "arm64"): next(item for item in TARGETS if item.endswith("-darwin"))}
    return targets[(platform.system(), platform.machine())]


def backup(config_path, destination):
    config = json.loads(config_path.read_text())
    data = Path(config["data_dir"])
    if not data.is_absolute():
        data = config_path.parent / data
    data = data.resolve()
    destination.mkdir(mode=0o700)
    shutil.copy2(config_path, destination / "config.json")
    for source in [data / "status.sqlite3", *data.glob("remote-hosts/*/status.sqlite3")]:
        if source.exists():
            output = destination / source.relative_to(data)
            output.parent.mkdir(parents=True, exist_ok=True)
            with sqlite3.connect(f"{source.as_uri()}?mode=ro", uri=True) as original:
                with sqlite3.connect(output) as copied:
                    original.backup(copied)
                    if copied.execute("PRAGMA integrity_check").fetchone() != ("ok",):
                        raise ValueError("Database backup failed integrity verification")
    return data


def probe(config_path):
    config = json.loads(config_path.read_text())
    address = f"http://127.0.0.1:{int(config.get('port', 8767))}/api/info"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(address, timeout=2) as response:
        return json.load(response)


def install_market(codex, repository, reference):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"]))
    if any(item["name"] == MARKET for item in markets["marketplaces"]):
        subprocess.run([codex, "plugin", "marketplace", "remove", MARKET],
                       check=True, stdout=subprocess.DEVNULL)
    subprocess.run([codex, "plugin", "marketplace", "add", repository, "--ref", reference, "--json"],
                   check=True, stdout=subprocess.DEVNULL)
    subprocess.run([codex, "plugin", "marketplace", "upgrade", MARKET, "--json"],
                   check=True, stdout=subprocess.DEVNULL)
    subprocess.run([codex, "plugin", "add", f"codex-session-board@{MARKET}", "--json"],
                   check=True, stdout=subprocess.DEVNULL)


def cached_package(version, revision):
    home = Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex")))
    package = home / "plugins/cache" / MARKET / "codex-session-board" / version
    metadata = json.loads((package / ".codex-marketplace-install.json").read_text())
    if metadata.get("source_type") != "git" or metadata.get("revision") != revision:
        raise ValueError("Installed plugin Git revision differs from the fetched release")
    return verify_package(package, installed=True)


def activate(package, config_path, state, codex, reload, data, backup_path):
    manifest = verify_package(package)
    binary = package / "bin/codex-session-board"
    record = {"phase": "stopped", "release": manifest, "package": str(package),
              "config": str(config_path), "backup": str(backup_path)}
    (state / "pending.json").write_text(json.dumps(record, indent=2) + "\n")
    with (state / "service.log").open("ab") as log:
        child = subprocess.Popen([str(binary), "--config", str(config_path), "serve"],
                                 cwd=state, stdin=subprocess.DEVNULL, stdout=log,
                                 stderr=log, start_new_session=True)
    for _ in range(100):
        status = child.poll()
        if status is not None and status != 0:
            raise RuntimeError("New board exited before becoming ready; inspect service.log")
        try:
            info = probe(config_path)
            if (info.get("app") == "codex-session-board"
                    and info["build"] == manifest["binary_sha256"] and info["version"] == manifest["version"]
                    and (info["pid"] == child.pid or status == 0) and Path(info["dataDir"]).resolve() == data):
                break
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    else:
        child.terminate()
        child.wait(timeout=10)
        raise RuntimeError("New board identity was not verified")
    record.update({"phase": "running", "runtime": info, "pid": info["pid"]})
    (state / "pending.json").write_text(json.dumps(record, indent=2) + "\n")
    if reload:
        record["mcp"] = reload_plugins(codex, state, manifest["version"])
    record.update({"phase": "complete" if reload else "awaiting_client_reload", "mcp_reloaded": reload})
    (state / "current.json").write_text(json.dumps(record, indent=2) + "\n")
    (state / "pending.json").unlink()
    return record


def update(repository, channel, revision, config_path, state, codex, reload=True, expected_source=None):
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (state / "update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        reference = revision or f"{channel}/{target()}"
        with tempfile.TemporaryDirectory(prefix="fetch-", dir=state) as temporary:
            checkout = Path(temporary)
            subprocess.run(["git", "init", "-q", str(checkout)], check=True)
            subprocess.run(["git", "fetch", "-q", "--depth=1", repository, reference],
                           cwd=checkout, check=True)
            subprocess.run(["git", "checkout", "-q", "--detach", "FETCH_HEAD"], cwd=checkout, check=True)
            manifest = verify_package(checkout)
            if manifest["target"] != target():
                raise ValueError("Release targets a different operating system or architecture")
            if not revision and expected_source and manifest["source_commit"] != expected_source:
                raise ValueError("Candidate for the expected source is not yet available")
            source_revision = command(["git", "rev-parse", "HEAD"], checkout)
            directory = state / "releases" / f"{manifest['version']}-{manifest['binary_sha256'][:12]}"
            if directory.exists():
                if verify_package(directory) != manifest:
                    raise ValueError("Existing release directory has a different identity")
            else:
                shutil.copytree(checkout, directory, ignore=shutil.ignore_patterns(".git"))
            if command([str(directory / "bin/codex-session-board"), "--version"]) != f"codex-session-board {manifest['version']}":
                raise ValueError("Release executable version differs")
            command([str(directory / "bin/codex-session-board"), "--config", str(config_path), "stop"])
            backup_path = state / f"backup-{time.time_ns()}"
            data = backup(config_path, backup_path)
            (state / "pending.json").write_text(json.dumps({
                "phase": "installing", "package": str(directory), "backup": str(backup_path),
                "reference": reference, "config": str(config_path),
            }, indent=2) + "\n")
            install_market(codex, repository, reference)
            cached = cached_package(manifest["version"], source_revision)
            if cached != manifest:
                raise ValueError("Installed plugin cache differs from fetched release")
            result = activate(directory, config_path, state, codex, reload, data, backup_path)
            result["revision"] = source_revision
            (state / "current.json").write_text(json.dumps(result, indent=2) + "\n")
            return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--channel", choices=("candidate", "stable"), default="stable")
    parser.add_argument("--revision", help="Explicit immutable release revision, including rollback")
    parser.add_argument("--expected-channel-source", help="Reject a channel still built from another source")
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--state", type=Path, default=Path.home() / ".local/share/codex-session-board/installation")
    parser.add_argument("--codex", default="codex")
    parser.add_argument("--defer-reload", action="store_true")
    parser.add_argument("--ssh-config", type=Path)
    arguments = parser.parse_args()
    if arguments.ssh_config:
        os.environ["GIT_SSH_COMMAND"] = shlex.join(["ssh", "-F", str(arguments.ssh_config.resolve())])
    result = update(arguments.repository, arguments.channel, arguments.revision,
                    arguments.config.resolve(), arguments.state.resolve(),
                    arguments.codex, not arguments.defer_reload, arguments.expected_channel_source)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
