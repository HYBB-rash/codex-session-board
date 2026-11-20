import argparse
import fcntl
import json
import os
import platform
import shutil
import shlex
import subprocess
import tempfile
from contextlib import contextmanager, nullcontext
from pathlib import Path

from plugin_package import TARGETS, verify_package
from reload_plugins import reload_plugins


MARKET = "session-board-rust"
COMMAND_TIMEOUT = 120


def command(arguments, root=None):
    return subprocess.check_output(
        arguments, cwd=root, text=True, stderr=subprocess.PIPE, timeout=COMMAND_TIMEOUT).strip()


def run(arguments, **kwargs):
    return subprocess.run(arguments, check=True, text=True, stderr=subprocess.PIPE,
                          timeout=COMMAND_TIMEOUT, **kwargs)


def target():
    targets = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
               ("Darwin", "arm64"): next(item for item in TARGETS if item.endswith("-darwin"))}
    return targets[(platform.system(), platform.machine())]


def market_registration(codex):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"]))
    selected = [item for item in markets["marketplaces"] if item["name"] == MARKET]
    if len(selected) > 1:
        raise ValueError("Installed marketplace is ambiguous")
    return selected[0] if selected else None


def fetch_snapshot(checkout, repository, reference):
    run(["git", "init", "-q", str(checkout)])
    run(["git", "fetch", "-q", "--depth=1", "--no-tags", repository, reference], cwd=checkout)
    run(["git", "checkout", "-q", "--detach", "FETCH_HEAD"], cwd=checkout)


def remove_market(codex):
    run([codex, "plugin", "marketplace", "remove", MARKET], stdout=subprocess.DEVNULL)


def add_market(codex, repository, revision=None, checkout=None):
    if checkout is None:
        run([codex, "plugin", "marketplace", "add", repository, "--json"], stdout=subprocess.DEVNULL)
        return
    source = repository.strip().rstrip("/")
    if source.startswith("https://github.com/") and not source.endswith(".git"):
        source += ".git"
    environment = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                       GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0=f"url.{checkout.as_uri()}.insteadOf",
                       GIT_CONFIG_VALUE_0=source, GIT_ALLOW_PROTOCOL="file")
    environment.pop("GIT_CONFIG_PARAMETERS", None)
    run([codex, "plugin", "marketplace", "add", source, "--json", "--ref", revision],
        env=environment, stdout=subprocess.DEVNULL)


def add_plugin(codex):
    run([codex, "plugin", "add", f"codex-session-board@{MARKET}", "--json"], stdout=subprocess.DEVNULL)


def install_market(codex, repository, revision, checkout, manifest):
    previous = market_registration(codex)
    with tempfile.TemporaryDirectory(prefix="restore-", dir=checkout.parent) as temporary:
        if previous:
            root = Path(previous["root"]).resolve()
            old_manifest = verify_package(root)
            old_source = previous["marketplaceSource"]
            old_repository = old_source["source"]
            old_revision = None
            backup = None
            if old_source["sourceType"] == "git":
                old_revision = command(["git", "rev-parse", "HEAD"], root)
                backup = Path(temporary) / "package"
                fetch_snapshot(backup, root.as_uri(), old_revision)
                if verify_package(backup) != old_manifest:
                    raise ValueError("Previous marketplace snapshot differs from its backup")
            elif old_source["sourceType"] != "local":
                raise ValueError("Cannot prepare recovery for this marketplace source")
        try:
            if previous:
                remove_market(codex)
            add_market(codex, repository, revision, checkout)
            add_plugin(codex)
            if cached_package(codex, manifest["version"], revision) != manifest:
                raise ValueError("Installed plugin cache differs from fetched release")
        except (OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
            try:
                if market_registration(codex):
                    remove_market(codex)
                if previous:
                    add_market(codex, old_repository, old_revision, backup)
                    add_plugin(codex)
                    restored = cached_package(codex, old_manifest["version"], old_revision) if old_revision else \
                        verify_package(cache_directory(old_manifest["version"]), installed=True)
                    if restored != old_manifest:
                        raise ValueError("Restored plugin cache differs from previous release")
            except (OSError, KeyError, ValueError, subprocess.SubprocessError) as restore_error:
                raise RuntimeError(f"{command_error(error)}。恢复旧市场失败：{command_error(restore_error)}。"
                                   "请按部署文档使用上次安装回执中的 revision 恢复。") from error
            recovery = "已恢复上一个插件市场及缓存" if previous else "本次未留下市场注册，原缓存可保留"
            raise RuntimeError(f"{command_error(error)}。{recovery}。") from error


def command_error(error):
    stderr = getattr(error, "stderr", None)
    if isinstance(stderr, bytes):
        stderr = stderr.decode(errors="replace")
    return stderr.strip()[:1000] if stderr and stderr.strip() else str(error)


def cache_directory(version):
    home = Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex")))
    return home / "plugins/cache" / MARKET / "codex-session-board" / version


def cached_package(codex, version, revision):
    selected = market_registration(codex)
    if selected is None or selected.get("marketplaceSource", {}).get("sourceType") != "git":
        raise ValueError("Installed marketplace is not the expected native Git snapshot")
    if command(["git", "rev-parse", "HEAD"], Path(selected["root"])) != revision:
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
    selected = market_registration(codex)
    if selected is None:
        raise ValueError("Installed marketplace is missing")
    path = Path(selected["root"]) / ".agents/plugins/marketplace.json"
    if not path.is_file():
        raise ValueError("Installed marketplace must provide a marketplace.json file")
    return path


@contextmanager
def update_lock(state):
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (state / "update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield lock


def matching_install(codex, manifest, revision, previous, running_build):
    if running_build is None:
        return False
    if (running_build != manifest["binary_sha256"]
            and not (previous and previous.get("installed") and previous.get("release") == manifest)):
        return False
    try:
        return cached_package(codex, manifest["version"], revision) == manifest
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        return False


def install_release(checkout, directory, codex, repository, record):
    manifest = record["release"]
    if directory.exists():
        if verify_package(directory) != manifest:
            raise ValueError("Existing release directory has a different identity")
    else:
        with tempfile.TemporaryDirectory(prefix=".release-", dir=directory.parent) as temporary:
            staged = Path(temporary) / "package"
            shutil.copytree(checkout, staged, ignore=shutil.ignore_patterns(".git"))
            staged.rename(directory)
    install_market(codex, repository, record["revision"], checkout, manifest)


def load_release(codex, state, record, reload):
    if reload:
        try:
            record["desktop"] = reload_plugins(
                codex, state, record["release"]["version"], marketplace_file(codex))
            record["desktop_loaded"] = record["desktop"]["desktop_loaded"]
            record["phase"] = record["desktop"]["phase"]
        except (OSError, ValueError, RuntimeError, ImportError, subprocess.SubprocessError) as error:
            record["reload_error"] = str(error)
    else:
        record["reload_deferred"] = True


def update(repository, channel, revision, state, codex, reload=True, expected_source=None,
           *, inherited_lock=None, progress=None, running_build=None):
    report = progress or (lambda phase, **fields: None)
    with update_lock(state) if inherited_lock is None else nullcontext():
        previous_path = state / "current.json"
        previous = json.loads(previous_path.read_text()) if previous_path.exists() else None
        reference = revision or f"refs/heads/{channel}/{target()}"
        report("fetching")
        with tempfile.TemporaryDirectory(prefix="fetch-", dir=state) as temporary:
            checkout = Path(temporary)
            fetch_snapshot(checkout, repository, reference)
            report("verifying")
            manifest = verify_package(checkout)
            if manifest["target"] != target():
                raise ValueError("Release targets a different operating system or architecture")
            if not revision and expected_source and manifest["source_commit"] != expected_source:
                raise ValueError("Candidate for the expected source is not yet available")
            source_revision = command(["git", "rev-parse", "HEAD"], checkout)
            directory = state / "releases" / f"{manifest['version']}-{manifest['binary_sha256'][:12]}"
            if command([str(checkout / "bin/codex-session-board"), "--version"]) != f"codex-session-board {manifest['version']}":
                raise ValueError("Release executable version differs")
            matched = matching_install(codex, manifest, source_revision, previous, running_build)
            if matched and running_build == manifest["binary_sha256"]:
                return {"phase": "up_to_date", "installed": True, "release": manifest, "revision": source_revision}
            record = {
                "phase": "installing", "installed": False, "desktop_loaded": False,
                "release": manifest, "package": str(directory), "reference": reference,
                "revision": source_revision,
            }
            if previous is not None:
                record["legacy_installation"] = previous.get("legacy_installation", previous)
            if not matched:
                directory.parent.mkdir(mode=0o700, exist_ok=True)
                write_record(state / "pending.json", record)
                report("installing", version=manifest["version"])
                install_release(checkout, directory, codex, repository, record)
            record.update(installed=True, phase="awaiting_desktop_reload")
            write_record(state / "pending.json", record)
            write_record(state / "current.json", record)
            report("reloading", version=manifest["version"])
            load_release(codex, state, record, reload)
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
