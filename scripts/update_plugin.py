import argparse
import fcntl
import json
import os
import platform
import shutil
import shlex
import subprocess
import tempfile
from pathlib import Path

from package_plugin import TARGETS, command, verify_package
from reload_plugins import reload_plugins


MARKET = "session-board-rust"


def target():
    targets = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
               ("Darwin", "arm64"): next(item for item in TARGETS if item.endswith("-darwin"))}
    return targets[(platform.system(), platform.machine())]


def install_market(codex, repository, reference):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"]))
    if any(item["name"] == MARKET for item in markets["marketplaces"]):
        subprocess.run([codex, "plugin", "marketplace", "remove", MARKET],
                       check=True, stdout=subprocess.DEVNULL)
    subprocess.run([codex, "plugin", "marketplace", "add", repository, "--ref", reference, "--json"],
                   check=True, stdout=subprocess.DEVNULL)
    subprocess.run([codex, "plugin", "add", f"codex-session-board@{MARKET}", "--json"],
                   check=True, stdout=subprocess.DEVNULL)


def cache_directory(version):
    home = Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex")))
    return home / "plugins/cache" / MARKET / "codex-session-board" / version


def cached_package(codex, version, revision):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"]))
    selected = [item for item in markets["marketplaces"] if item["name"] == MARKET]
    if len(selected) != 1 or selected[0].get("marketplaceSource", {}).get("sourceType") != "git":
        raise ValueError("Installed marketplace is not the expected native Git snapshot")
    if command(["git", "rev-parse", "HEAD"], Path(selected[0]["root"])) != revision:
        raise ValueError("Native marketplace Git revision differs from the fetched release")
    package = cache_directory(version)
    metadata_path = package / ".codex-marketplace-install.json"
    if metadata_path.exists():
        metadata = json.loads(metadata_path.read_text())
        if metadata.get("source_type") != "git" or metadata.get("revision") != revision:
            raise ValueError("Installed plugin Git revision differs from the fetched release")
    return verify_package(package, installed=True)


def write_record(path, record):
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            json.dump(record, output, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


def marketplace_file(codex):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"]))
    selected = [item for item in markets["marketplaces"] if item["name"] == MARKET]
    if len(selected) != 1:
        raise ValueError("Installed marketplace is missing or ambiguous")
    path = Path(selected[0]["root"]) / ".agents/plugins/marketplace.json"
    if not path.is_file():
        raise ValueError("Installed marketplace must provide a marketplace.json file")
    return path


def update(repository, channel, revision, state, codex, reload=True, expected_source=None):
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (state / "update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        previous_path = state / "current.json"
        previous = json.loads(previous_path.read_text()) if previous_path.exists() else None
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
            record = {
                "phase": "installing", "installed": False, "desktop_loaded": False,
                "release": manifest, "package": str(directory), "reference": reference,
                "revision": source_revision,
            }
            if previous is not None:
                record["legacy_installation"] = previous.get("legacy_installation", previous)
            write_record(state / "pending.json", record)
            install_market(codex, repository, source_revision)
            cached = cached_package(codex, manifest["version"], source_revision)
            if cached != manifest:
                raise ValueError("Installed plugin cache differs from fetched release")
            record.update(installed=True, phase="awaiting_desktop_reload")
            write_record(state / "pending.json", record)
            write_record(state / "current.json", record)
            if reload:
                try:
                    record["desktop"] = reload_plugins(
                        codex, state, manifest["version"], marketplace_file(codex))
                    record["desktop_loaded"] = record["desktop"]["desktop_loaded"]
                    record["phase"] = record["desktop"]["phase"]
                except (OSError, ValueError, RuntimeError, ImportError, subprocess.SubprocessError) as error:
                    record["reload_error"] = str(error)
            else:
                record["reload_deferred"] = True
            write_record(state / "current.json", record)
            (state / "pending.json").unlink()
            return record


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--channel", choices=("candidate", "stable"), default="stable")
    parser.add_argument("--revision", help="Explicit immutable release revision, including rollback")
    parser.add_argument("--expected-channel-source", help="Reject a channel still built from another source")
    data_home = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    parser.add_argument("--state", type=Path, default=data_home / "codex-session-board/installation")
    parser.add_argument("--codex", default="codex")
    parser.add_argument("--defer-reload", action="store_true")
    parser.add_argument("--ssh-config", type=Path)
    arguments = parser.parse_args()
    if arguments.ssh_config:
        os.environ["GIT_SSH_COMMAND"] = shlex.join(["ssh", "-F", str(arguments.ssh_config.resolve())])
    result = update(arguments.repository, arguments.channel, arguments.revision,
                    arguments.state.resolve(),
                    arguments.codex, not arguments.defer_reload, arguments.expected_channel_source)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
