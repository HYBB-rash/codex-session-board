import hashlib
import json
import re
import tarfile
from pathlib import Path, PurePosixPath


BASELINE = "2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc"
CONTRACT = "codex-session-board/acceptance-v3"
FROZEN = {
    "requirements.json": "5b20b6135b59c3daf4002bea17316c0fbaf80228cd822e24ffb9853a4d69c32b",
    "model.py": "afc587078c415ccb631fed4efdcef304269096fbfece812e680a6d4e195738d7",
}
PLATFORMS = {"linux-x86_64": ("linux", "x86_64"), "macos-arm64": ("darwin", "arm64")}
SOURCE_DIRECTORIES = {"acceptance", "scripts", "src", "ui"}
PLUGIN_MANIFEST = ".codex-plugin/plugin.json"
SOURCE_FILES = {
    "README.md", "package.json", "package-lock.json", "tsconfig.json", "biome.json",
    PLUGIN_MANIFEST, ".mcp.json",
}
VERSION_FILE = "board-assets/version.txt"
RUNTIME_REQUIRED = {"bin/codex-enhancer.js", "runtime/main.js", "runtime/preload.js"}


def need(value, reason):
    if not value:
        raise ValueError(reason)


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, f"重复的 JSON 字段 {key}")
        result[key] = value
    return result


def parse(raw):
    return json.loads(raw, object_pairs_hook=object_pairs)


def json_file(path):
    return parse(path.read_bytes())


def stream_sha256(stream):
    digest = hashlib.sha256()
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
    return digest.hexdigest()


def sha256(path):
    with path.open("rb") as stream:
        return stream_sha256(stream)


def plugin_identity(raw):
    manifest = parse(raw)
    need(isinstance(manifest, dict), "插件清单必须是 JSON 对象")
    version = manifest.pop("version", None)
    need(isinstance(version, str), "插件清单缺少版本")
    normalized = json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest(), version


def sha_format(value):
    return isinstance(value, str) and re.fullmatch(r"[a-f0-9]{64}", value) is not None


def fingerprint(files):
    need(files, "缺少可计算指纹的文件")
    accumulator = hashlib.sha256()
    for name, digest in sorted(files.items()):
        accumulator.update(name.encode("utf-8") + b"\0" + bytes.fromhex(digest))
    return accumulator.hexdigest()


def relative_file(name):
    need(isinstance(name, str) and "\\" not in name and "\0" not in name, "路径格式无效")
    path = PurePosixPath(name)
    need(
        path.parts and ".." not in path.parts and not path.is_absolute() and path.as_posix() == name,
        f"路径越界或不规范 {name}",
    )
    return path


def evidence_file(root, name):
    relative = relative_file(name)
    path = root
    for component in relative.parts:
        path /= component
        need(not path.is_symlink(), f"不允许证据路径使用符号链接 {name}")
    need(path.is_file() and path.resolve().is_relative_to(root), f"证据文件不存在 {name}")
    return path


def contract_at(root):
    acceptance = root / "acceptance"
    lock = json_file(acceptance / "contract.lock.json")
    need(lock == {"schema_version": 1, "files": FROZEN}, "冻结契约锁与上游不一致")
    for name, digest in FROZEN.items():
        need(sha256(acceptance / name) == digest, f"冻结文件已改变 {name}")
    contract = json_file(acceptance / "requirements.json")
    need(contract["schema_version"] == 1 and contract["contract_id"] == CONTRACT, "冻结契约版本错误")
    ids = [item["id"] for item in contract["requirements"]]
    need(len(ids) == 31 and len(set(ids)) == 31, "冻结契约应有 31 个唯一需求 ID")
    return contract["requirements"]


def source_path(name):
    parts = PurePosixPath(name).parts
    return (
        name in SOURCE_FILES
        or (
            bool(parts)
            and parts[0] in SOURCE_DIRECTORIES
            and not ({"node_modules", "__pycache__", "dist"} & set(parts))
            and not any(part.startswith(".") for part in parts[1:])
        )
    )


def tree_files(root, names, include):
    files = {}
    for name in names:
        selected = root / name
        need(selected.exists(), f"缺少指纹路径 {name}")
        need(not selected.is_symlink(), f"指纹路径包含符号链接 {name}")
        candidates = (selected,) if selected.is_file() else selected.rglob("*")
        for path in candidates:
            relative = path.relative_to(root).as_posix()
            if not include(relative):
                continue
            need(not path.is_symlink(), f"指纹路径包含符号链接 {relative}")
            if path.is_file():
                files[relative] = (
                    plugin_identity(path.read_bytes())[0] if relative == PLUGIN_MANIFEST
                    else sha256(path)
                )
            else:
                need(path.is_dir(), f"指纹路径包含非常规文件 {relative}")
    return files


def candidate_files(root):
    source = tree_files(root, SOURCE_DIRECTORIES | SOURCE_FILES, source_path)
    runtime = tree_files(root / "dist", [Path(".")], lambda _: True)
    need(RUNTIME_REQUIRED.issubset(runtime), "本地构建缺少运行入口，请先运行 npm run build")
    return source, runtime


def open_archive(handle):
    with tarfile.open(fileobj=handle, mode="r:gz") as archive:
        files = {}
        members = set()
        roots = set()
        manifest = None
        version = None
        plugin_version = None
        for member in archive:
            name = relative_file(member.name)
            need(name not in members, f"归档路径重复 {name}")
            members.add(name)
            roots.add(name.parts[0])
            need(member.isfile() or member.isdir(), f"归档包含非常规文件 {name}")
            if member.isdir():
                continue
            need(len(name.parts) > 1, f"归档缺少单一根目录 {name}")
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            with archive.extractfile(member) as stream:
                if relative == "release.json":
                    manifest = parse(stream.read())
                elif relative == PLUGIN_MANIFEST:
                    files[relative], plugin_version = plugin_identity(stream.read())
                elif relative == f"dist/{VERSION_FILE}":
                    version = stream.read()
                    files[relative] = hashlib.sha256(version).hexdigest()
                else:
                    files[relative] = stream_sha256(stream)
        need(len(roots) == 1 and manifest is not None, "发行归档缺少唯一根目录或 release.json")
        return manifest, files, version, plugin_version


def archive_identity(path, platform, source, runtime):
    need(path.is_file() and not path.is_symlink(), "发行归档必须是普通文件")
    with path.open("rb") as handle:
        compressed = stream_sha256(handle)
        handle.seek(0)
        manifest, files, packaged_version, plugin_version = open_archive(handle)
    need(manifest["schema"] == 1 and manifest["platform"] == platform, f"归档平台范围不符 {platform}")
    version = manifest["version"]
    need(
        isinstance(version, str) and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[\w.-]+)?", version),
        "归档版本无效",
    )
    expected = {}
    for entry in manifest["files"]:
        name = relative_file(entry["path"]).as_posix()
        digest = entry["sha256"]
        need(name not in expected and sha_format(digest), f"归档运行文件项无效 {name}")
        expected[name] = digest
    archived_source = {name: digest for name, digest in files.items() if source_path(name)}
    archived_runtime = {name[5:]: digest for name, digest in files.items() if name.startswith("dist/")}
    need(expected == archived_runtime, f"归档运行文件与 SHA256 清单不符 {platform}")
    need(source == archived_source, f"归档源码与当前候选不同 {platform}")
    need(
        {name: digest for name, digest in archived_runtime.items() if name != VERSION_FILE}
        == {name: digest for name, digest in runtime.items() if name != VERSION_FILE},
        f"归档运行代码与本次自动测试产物不同 {platform}",
    )
    need(packaged_version == f"{version}\n".encode("utf-8"), "归档运行版本与发行清单不符")
    if PLUGIN_MANIFEST in source:
        need(plugin_version == version, "归档插件版本与发行清单不符")
    return {
        "version": version,
        "archiveSha256": compressed,
        "runtimeSha256": fingerprint(archived_runtime),
    }


def identify(root, archives):
    source, runtime = candidate_files(root)
    releases = {}
    errors = {}
    for platform, path in archives.items():
        try:
            releases[platform] = archive_identity(path, platform, source, runtime)
        except (OSError, KeyError, ValueError, TypeError, tarfile.TarError) as error:
            errors[platform] = str(error)
    versions = {item["version"] for item in releases.values()}
    need(len(versions) <= 1, "两个平台的发行版本不一致")
    return {
        "contract": {"id": CONTRACT, "sha256": FROZEN["requirements.json"], "baseline": BASELINE},
        "candidate": {"sourceSha256": fingerprint(source), "testRuntimeSha256": fingerprint(runtime)},
        "archives": releases,
        "archiveErrors": errors,
    }
