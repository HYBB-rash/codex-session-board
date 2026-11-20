import argparse
import fcntl
import json
import os
import platform
import shutil
import shlex
import stat
import subprocess
import tempfile
from codecs import BOM_UTF8
from contextlib import contextmanager, nullcontext
from pathlib import Path

import tomlkit

from plugin_package import TARGETS, verify_package
from plugin_publication import Publication, PublicationConflict, fingerprint
from reload_plugins import reload_plugins


MARKET = "session-board-rust"
PLUGIN = "codex-session-board"
PLUGIN_ID = f"{PLUGIN}@{MARKET}"
COMMAND_TIMEOUT = 120


def command(arguments, root=None, *, home=None):
    return subprocess.check_output(
        arguments, cwd=root, text=True, stderr=subprocess.PIPE, timeout=COMMAND_TIMEOUT,
        env=native_environment(home)).strip()


def run(arguments, **kwargs):
    return subprocess.run(arguments, check=True, text=True, stderr=subprocess.PIPE,
                          timeout=COMMAND_TIMEOUT, **kwargs)


def target():
    targets = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
               ("Darwin", "arm64"): next(item for item in TARGETS if item.endswith("-darwin"))}
    return targets[(platform.system(), platform.machine())]


def market_registration(codex, *, home=None):
    markets = json.loads(command([codex, "plugin", "marketplace", "list", "--json"], home=home))
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


def market_source(repository):
    source = repository.strip().rstrip("/")
    if source.startswith("https://github.com/") and not source.endswith(".git"):
        source += ".git"
    return source


def native_environment(home):
    return dict(os.environ, CODEX_HOME=str(home)) if home is not None else None


def add_market(codex, repository, revision=None, checkout=None, *, home=None):
    if checkout is None:
        run([codex, "plugin", "marketplace", "add", repository, "--json"],
            env=native_environment(home), stdout=subprocess.DEVNULL)
        return
    repository = market_source(repository)
    environment = dict(native_environment(home) or os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                       GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0=f"url.{checkout.as_uri()}.insteadOf",
                       GIT_CONFIG_VALUE_0=repository, GIT_ALLOW_PROTOCOL="file")
    environment.pop("GIT_CONFIG_PARAMETERS", None)
    arguments = [codex, "plugin", "marketplace", "add", repository, "--json"]
    if revision is not None:
        arguments.extend(["--ref", revision])
    run(arguments, env=environment, stdout=subprocess.DEVNULL)


def add_plugin(codex, home=None):
    run([codex, "plugin", "add", PLUGIN_ID, "--json"], env=native_environment(home),
        stdout=subprocess.DEVNULL)


def git_revision(root):
    return command(["git", "rev-parse", "HEAD"], root)


def config_bytes(path):
    try:
        return path.read_bytes()
    except FileNotFoundError:
        return b""


def market_settings(document):
    settings = document.get("marketplaces", {}).get(MARKET)
    return settings.unwrap() if settings is not None else None


def owned_market(registered, settings, source, revision):
    return (registered is not None
            and registered["marketplaceSource"] == {"sourceType": "git", "source": source}
            and settings is not None and settings.get("source_type") == "git"
            and settings.get("source") == source and settings.get("ref") == revision and not settings.get("sparse_paths")
            and git_revision(Path(registered["root"])) == revision)


def read_config(path):
    return tomlkit.parse(config_bytes(path).decode("utf-8-sig"))


def plugin_enabled(config):
    return config.get("plugins", {}).get(PLUGIN_ID, {}).get("enabled")


def verify_candidate_cache(codex, manifest, revision, *, home=None):
    if cached_package(codex, manifest["version"], revision, home=home) != manifest:
        raise ValueError("Installed plugin cache differs from fetched release")


def stage_candidate(codex, source, revision, checkout, manifest, cache, private, original):
    for name in ("requirements.toml", "managed_config.toml"):
        policy = codex_home() / name
        if policy.is_file():
            (private / name).symlink_to(policy.resolve())
    features = {"features": original["features"]} if "features" in original else {}
    config = private / "config.toml"
    config.write_text(tomlkit.dumps(features))
    add_market(codex, source, revision, checkout, home=private)
    candidate = market_registration(codex, home=private)
    settings = market_settings(read_config(config))
    if not owned_market(candidate, settings, source, revision):
        raise ValueError("未确认本次候选市场的来源和提交")
    if cache.exists():
        shutil.copytree(cache, cache_directory(home=private), symlinks=True)
    add_plugin(codex, private)
    verify_candidate_cache(codex, manifest, revision, home=private)
    enabled = plugin_enabled(read_config(config))
    if enabled is not True:
        raise ValueError("未确认原生安装写入的插件启用状态")
    return Path(candidate["root"]), settings


def require_original_config(current, original):
    if market_settings(current) != market_settings(original):
        raise PublicationConflict("市场注册冲突：已保留当前来源，请核对后重试")
    if plugin_enabled(current) != plugin_enabled(original):
        raise PublicationConflict("插件启用状态冲突：已保留当前配置，请在 Codex 插件管理中核对开关")


def require_config_target(logical, resolved):
    if logical.resolve() != resolved:
        raise PublicationConflict("Codex 配置冲突：配置文件路径已变化")


def require_install_targets(logical_market, market, logical_cache, cache):
    if logical_market.resolve() != market or logical_cache.resolve() != cache:
        raise PublicationConflict("市场或插件缓存冲突：安装目录已变化")


def install_market(codex, repository, revision, checkout, manifest):
    logical_config = codex_home() / "config.toml"
    home = logical_config.parent.resolve()
    config = logical_config.resolve()
    original = read_config(config)
    previous = market_registration(codex)
    logical_market = home / ".tmp/marketplaces" / MARKET
    logical_cache = cache_directory()
    if logical_market.is_symlink() or logical_cache.is_symlink():
        raise ValueError("Cannot update a symlinked marketplace or plugin cache")
    market = logical_market.resolve()
    cache = logical_cache.resolve()
    old_market, old_cache = fingerprint(market), fingerprint(cache)
    if previous:
        prior_source = previous["marketplaceSource"]
        settings = market_settings(original)
        if not settings or settings.get("source_type") != prior_source["sourceType"] \
                or settings.get("source") != prior_source["source"]:
            raise PublicationConflict("市场注册冲突：现有市场不由当前用户配置登记")
        if prior_source["sourceType"] == "git" and Path(previous["root"]).resolve() != market:
            raise PublicationConflict("市场注册冲突：现有 Git 市场路径与安装入口不一致")
    cache.parent.mkdir(parents=True, exist_ok=True)
    backup = Path(tempfile.mkdtemp(prefix=".board-rollback-", dir=cache.parent))
    disposable = [backup]
    config_backup = None
    config_publication = None
    publications = []
    source = market_source(repository)
    try:
        private = backup / "native"
        private.mkdir()
        staged_market, settings = stage_candidate(codex, source, revision, checkout, manifest, cache, private, original)
        market.parent.mkdir(parents=True, exist_ok=True)
        if staged_market.stat().st_dev != market.parent.stat().st_dev:
            market_backup = Path(tempfile.mkdtemp(prefix=".board-rollback-market-", dir=market.parent))
            disposable.append(market_backup)
            staged_market = shutil.copytree(staged_market, market_backup / MARKET, symlinks=True)
        prior_config = fingerprint(config)
        contents = config_bytes(config)
        current = tomlkit.parse(contents.decode("utf-8-sig"))
        require_original_config(current, original)
        current.setdefault("marketplaces", {})[MARKET] = settings
        if plugin_enabled(original) is None:
            current.setdefault("plugins", {}).setdefault(PLUGIN_ID, {})["enabled"] = True
        config.parent.mkdir(parents=True, exist_ok=True)
        config_backup = Path(tempfile.mkdtemp(prefix=".board-rollback-config-", dir=config.parent))
        staged_config = config_backup / "config.toml"
        bom = "\ufeff" if contents.startswith(BOM_UTF8) else ""
        write_atomic(staged_config, bom + tomlkit.dumps(current))
        if prior_config is not None:
            staged_config.chmod(stat.S_IMODE(config.stat().st_mode))
        config_publication = Publication(config, staged_config, prior_config, "Codex 配置")
        publications.extend([
            Publication(market, staged_market, old_market, "市场检出"),
            Publication(cache, cache_directory(home=private), old_cache, "插件缓存"),
            config_publication,
        ])
        require_config_target(logical_config, config)
        require_install_targets(logical_market, market, logical_cache, cache)
        for publication in publications:
            publication.publish()
        verify_candidate_cache(codex, manifest, revision)
        require_config_target(logical_config, config)
        registered = market_registration(codex)
        active_settings = market_settings(read_config(logical_config))
        if not owned_market(registered, active_settings, source, revision):
            raise PublicationConflict("市场注册冲突：实际配置中的候选来源或提交不匹配")
        for publication in publications:
            publication.verify()
        require_config_target(logical_config, config)
        require_install_targets(logical_market, market, logical_cache, cache)
    except (Exception, KeyboardInterrupt) as error:
        failures = []
        for publication in reversed(publications):
            try:
                publication.restore()
            except (Exception, KeyboardInterrupt) as restore_error:
                failures.append(command_error(restore_error))
                if publication.live == config or isinstance(restore_error, KeyboardInterrupt):
                    break
        try:
            require_original_config(read_config(logical_config), original)
            if fingerprint(market) != old_market or fingerprint(cache) != old_cache:
                raise PublicationConflict("市场或插件缓存冲突：检测到其他安装写入")
        except (Exception, KeyboardInterrupt) as conflict:
            failures.append(command_error(conflict))
        if failures or isinstance(error, PublicationConflict):
            detail = f"。{'；'.join(failures)}" if failures else ""
            locations = "、".join(map(str, disposable + ([config_backup] if config_backup else [])))
            raise RuntimeError(f"{command_error(error)}{detail}。"
                               f"备份保留在 {locations}。请按部署文档恢复。") from error
        recovery = "已恢复上一个插件市场" if previous else "本次未留下市场注册"
        if old_cache is not None:
            recovery += "及缓存" if previous else "，已恢复原缓存"
        if config_publication is not None and config_publication.applied:
            recovery += f"。换下的配置保留在 {config_publication.staged}，请按部署文档核对"
        elif config_backup is not None:
            disposable.append(config_backup)
        for directory in disposable:
            try:
                shutil.rmtree(directory)
            except (OSError, KeyboardInterrupt) as cleanup_error:
                recovery += f"。清理 {directory} 中断：{command_error(cleanup_error)}"
                if isinstance(cleanup_error, KeyboardInterrupt):
                    break
        raise RuntimeError(f"{command_error(error)}。{recovery}。") from error
    if prior_config is None:
        disposable.append(config_backup)
    return disposable


def command_error(error):
    if isinstance(error, KeyboardInterrupt):
        return "更新中断（SIGINT）"
    stderr = getattr(error, "stderr", None)
    if isinstance(stderr, bytes):
        stderr = stderr.decode(errors="replace")
    return stderr.strip()[:1000] if stderr and stderr.strip() else str(error)


def codex_home():
    return Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")


def cache_directory(version=None, *, home=None):
    root = (home if home is not None else codex_home()) / "plugins/cache" / MARKET / PLUGIN
    return root / version if version is not None else root


def cached_package(codex, version, revision, *, home=None):
    selected = market_registration(codex, home=home)
    if selected is None or selected.get("marketplaceSource", {}).get("sourceType") != "git":
        raise ValueError("Installed marketplace is not the expected native Git snapshot")
    if command(["git", "rev-parse", "HEAD"], Path(selected["root"])) != revision:
        raise ValueError("Native marketplace Git revision differs from the fetched release")
    package = cache_directory(version, home=home)
    metadata_path = package / ".codex-marketplace-install.json"
    if metadata_path.exists():
        metadata = json.loads(metadata_path.read_text())
        if not isinstance(metadata, dict) or metadata.get("source_type") != "git" or metadata.get("revision") != revision:
            raise ValueError("Installed plugin Git revision differs from the fetched release")
    return verify_package(package, installed=True)


def write_record(path, record):
    write_atomic(path, json.dumps(record, indent=2) + "\n")


def write_atomic(path, contents):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(contents)
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


def matching_install(codex, manifest, revision, source, previous, running_build):
    if running_build is None:
        return False
    if (running_build != manifest["binary_sha256"]
            and not (previous and previous.get("installed") and previous.get("release") == manifest)):
        return False
    try:
        registered = market_registration(codex)
        settings = market_settings(read_config(codex_home() / "config.toml"))
        return (owned_market(registered, settings, source, revision)
                and cached_package(codex, manifest["version"], revision) == manifest)
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
    return install_market(codex, repository, record["revision"], checkout, manifest)


def load_release(codex, state, record, reload):
    if reload:
        try:
            record["desktop"] = reload_plugins(codex, state, record["release"]["version"])
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
        pending_path = state / "pending.json"
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
            record = {
                "phase": "installing", "installed": False, "desktop_loaded": False,
                "release": manifest, "package": str(directory), "reference": reference,
                "revision": source_revision,
            }
            if previous is not None:
                record["legacy_installation"] = previous.get("legacy_installation", previous)
            matched = matching_install(codex, manifest, source_revision, market_source(repository), previous, running_build)
            if matched and running_build == manifest["binary_sha256"]:
                record.update(phase="up_to_date", installed=True)
                if not previous or not previous.get("installed") or (
                        previous.get("revision"), previous.get("release")) != (source_revision, manifest):
                    write_record(previous_path, record)
                pending_path.unlink(missing_ok=True)
                return record
            disposable = []
            if not matched:
                directory.parent.mkdir(mode=0o700, exist_ok=True)
                write_record(pending_path, record)
                report("installing", version=manifest["version"])
                disposable = install_release(checkout, directory, codex, repository, record)
            record.update(installed=True, phase="awaiting_desktop_reload")
            try:
                write_record(previous_path, record)
                pending_path.unlink(missing_ok=True)
                for backup in disposable:
                    try:
                        shutil.rmtree(backup)
                    except OSError as error:
                        record.setdefault("cleanup_errors", []).append(command_error(error))
                report("reloading", version=manifest["version"])
                load_release(codex, state, record, reload)
            except KeyboardInterrupt:
                record["reload_error"] = "已安装。更新收尾被中断，请重载 Codex 后重新打开看板"
                pending_path.unlink(missing_ok=True)
            write_record(previous_path, record)
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
