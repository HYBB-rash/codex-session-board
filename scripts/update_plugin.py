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
from dataclasses import dataclass
from pathlib import Path

import tomlkit

from plugin_package import TARGETS, verify_package
from reload_plugins import reload_plugins


MARKET = "session-board-rust"
PLUGIN = "codex-session-board"
PLUGIN_ID = f"{PLUGIN}@{MARKET}"
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
    run([codex, "plugin", "add", PLUGIN_ID, "--json"], stdout=subprocess.DEVNULL)


@dataclass
class GitMarketBackup:
    root: Path
    copy: Path
    revision: str
    settings: dict


def git_revision(root):
    return command(["git", "rev-parse", "HEAD"], root)


def backup_market(previous, backup, settings):
    if previous is None:
        return None
    source = previous["marketplaceSource"]
    if source["sourceType"] == "local":
        if not Path(source["source"]).is_dir():
            raise ValueError("Previous local marketplace source is missing")
        return None
    if source["sourceType"] != "git":
        raise ValueError("Cannot prepare recovery for this marketplace source")
    if settings is None:
        raise ValueError("Git marketplace is missing its user registration")
    root = Path(previous["root"]).resolve(strict=True)
    if not (root / ".git").is_dir() or (root / ".git").is_symlink():
        raise ValueError("Marketplace checkout does not have a self-contained Git directory")
    revision = git_revision(root)
    shutil.copytree(root, backup, symlinks=True)
    compare_market(root, backup)
    if git_revision(backup) != revision:
        raise ValueError("Previous marketplace revision differs from its backup")
    (backup.parent / "market-registration.toml").write_text(tomlkit.dumps({"marketplaces": {MARKET: settings}}))
    return GitMarketBackup(root, backup, revision, settings)


def compare_market(original, copied):
    run(["git", "diff", "--no-index", "--quiet", "--no-ext-diff", "--", str(original), str(copied)])


def config_text(path):
    try:
        return path.read_bytes().decode("utf-8")
    except FileNotFoundError:
        return ""


def market_settings(document):
    settings = document.get("marketplaces", {}).get(MARKET)
    return settings.unwrap() if settings is not None else None


def patch_config(path, edit):
    for _ in range(5):
        original = config_text(path)
        document = tomlkit.parse(original)
        if not edit(document) or write_atomic(path, tomlkit.dumps(document), expected=original):
            return
    raise RuntimeError("User config kept changing during rollback")


def restore_git_market(backup, config_path):
    def restore_registration(document):
        existing = market_settings(document)
        if existing == backup.settings:
            return False
        if existing is not None:
            raise ValueError("Marketplace registration changed during rollback")
        document.setdefault("marketplaces", {})[MARKET] = backup.settings
        return True

    with tempfile.TemporaryDirectory(prefix=".board-market-", dir=backup.root.parent) as temporary:
        staged = Path(temporary) / "market"
        shutil.copytree(backup.copy, staged, symlinks=True)
        compare_market(backup.copy, staged)
        staged.rename(backup.root)
    patch_config(config_path, restore_registration)


def restore_market(codex, previous, backup, config_path):
    registered = market_registration(codex)
    if registered == previous and backup is None:
        return
    if registered == previous and backup and market_settings(tomlkit.parse(config_text(config_path))) == backup.settings:
        try:
            if git_revision(backup.root) == backup.revision:
                compare_market(backup.copy, backup.root)
                return
        except (OSError, subprocess.SubprocessError):
            pass
    if registered:
        remove_market(codex)
    if backup:
        restore_git_market(backup, config_path)
        if (git_revision(backup.root) != backup.revision
                or market_settings(tomlkit.parse(config_text(config_path))) != backup.settings):
            raise ValueError("Restored marketplace revision or registration settings differ")
    elif previous:
        add_market(codex, previous["marketplaceSource"]["source"])
    if market_registration(codex) != previous:
        raise ValueError("Restored marketplace registration differs")


def restore_enabled(config, previous):
    plugins = config.get("plugins", {})
    plugin = plugins.get(PLUGIN_ID)
    enabled = previous.get("enabled") if previous is not None else None
    if plugin is None and previous is None:
        return False
    if plugin is None:
        raise ValueError("Plugin config was removed during installation")
    if plugin.get("enabled") == enabled:
        return False
    if plugin.get("enabled") is not True:
        raise ValueError("Plugin enablement changed during installation")
    if enabled is None:
        del plugin["enabled"]
        if previous is None and not plugin:
            del plugins[PLUGIN_ID]
    else:
        plugin["enabled"] = enabled
    return True


def restore_cache(cache, backup):
    saved = backup / "cache"
    staged = backup / "restored-cache"
    if saved.exists():
        shutil.copytree(saved, staged, symlinks=True)
    if cache.exists():
        cache.rename(backup / "failed-cache")
    if staged.exists():
        staged.rename(cache)


def install_market(codex, repository, revision, checkout, manifest):
    previous = market_registration(codex)
    cache = cache_directory()
    had_cache = cache.exists()
    if cache.is_symlink():
        raise ValueError("Cannot back up a symlinked plugin cache")
    config = (codex_home() / "config.toml").resolve()
    original = tomlkit.parse(config_text(config))
    previous_settings = original.get("plugins", {}).get(PLUGIN_ID)
    cache.parent.mkdir(parents=True, exist_ok=True)
    backup = Path(tempfile.mkdtemp(prefix=".board-rollback-", dir=cache.parent))
    keep_backup = False
    try:
        old_market = backup_market(previous, backup / "market", market_settings(original))
        if had_cache:
            shutil.copytree(cache, backup / "cache", symlinks=True)
        installing_plugin = False
        keep_backup = True
        try:
            if previous:
                remove_market(codex)
            add_market(codex, repository, revision, checkout)
            installing_plugin = True
            add_plugin(codex)
            if cached_package(codex, manifest["version"], revision) != manifest:
                raise ValueError("Installed plugin cache differs from fetched release")
        except Exception as error:
            actions = [("市场注册", lambda: restore_market(codex, previous, old_market, config))]
            if installing_plugin:
                actions.extend([("插件缓存", lambda: restore_cache(cache, backup)),
                                ("插件启用状态", lambda: patch_config(
                                    config, lambda document: restore_enabled(document, previous_settings)))])
            failures = []
            for name, action in actions:
                try:
                    action()
                except Exception as restore_error:
                    failures.append(f"{name}：{command_error(restore_error)}")
            if failures:
                raise RuntimeError(f"{command_error(error)}。恢复失败：{'；'.join(failures)}。"
                                   f"备份保留在 {backup}。请按部署文档恢复。") from error
            recovery = "已恢复上一个插件市场" if previous else "本次未留下市场注册"
            if had_cache:
                recovery += "及缓存" if previous else "，已恢复原缓存"
            keep_backup = False
            raise RuntimeError(f"{command_error(error)}。{recovery}。") from error
        else:
            keep_backup = False
    finally:
        if not keep_backup:
            shutil.rmtree(backup)


def command_error(error):
    stderr = getattr(error, "stderr", None)
    if isinstance(stderr, bytes):
        stderr = stderr.decode(errors="replace")
    return stderr.strip()[:1000] if stderr and stderr.strip() else str(error)


def codex_home():
    return Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")


def cache_directory(version=None):
    root = codex_home() / "plugins/cache" / MARKET / PLUGIN
    return root / version if version is not None else root


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
        if not isinstance(metadata, dict) or metadata.get("source_type") != "git" or metadata.get("revision") != revision:
            raise ValueError("Installed plugin Git revision differs from the fetched release")
    return verify_package(package, installed=True)


def write_record(path, record):
    write_atomic(path, json.dumps(record, indent=2) + "\n")


def write_atomic(path, contents, *, expected=None):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
            if expected is not None and config_text(path) != expected:
                return False
            os.replace(temporary, path)
            return True
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
