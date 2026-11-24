import re

from release_identity import PLATFORMS, json_file, need, sha256, sha_format


def require_version(version):
    need(re.fullmatch(r"\d+\.\d+\.\d+(?:-[A-Za-z0-9_.-]+)?", version, re.ASCII), "版本格式无效")
    return version


def checked_archive(path, expected=None):
    need(path.is_file() and not path.is_symlink(), f"候选归档必须是普通文件：{path}")
    checksum = path.with_name(path.name + ".sha256")
    observed = sha256(path)
    need(checksum.read_text(encoding="utf-8").strip() == f"{observed}  {path.name}"
         and (expected is None or observed == expected), f"候选包 SHA-256 校验失败：{path}")
    return path, observed


def candidate_identity(state, version, manifest=None):
    require_version(version)
    default = state / "candidates" / version / "candidate.json"
    source = manifest or (default if default.is_file() else state / f"release-gate-{version}.json")
    identity = json_file(source)
    need(identity["schemaVersion"] == 1 and identity.get("version", version) == version
         and not identity.get("archiveErrors"), "候选清单版本或归档状态无效")
    need(all(sha_format(identity["candidate"][field]) for field in ("sourceSha256", "testRuntimeSha256")),
         "候选源码或测试运行文件摘要无效")
    if "commit" in identity:
        log = source.parent / "build.log"
        need(identity.get("buildLog") == log.name and log.is_file() and not log.is_symlink()
             and sha256(log) == identity.get("buildLogSha256"), "构建日志与候选清单的摘要不符")
    need(set(identity["archives"]) == set(PLATFORMS), "候选需要双平台归档")
    paths = {}
    for platform, entry in identity["archives"].items():
        name = f"session-board-{version}-{platform}.tar.gz"
        need(entry["version"] == version and entry.get("file", name) == name
             and not any(entry["deliveryChecks"].values()), f"归档身份或交付检查无效：{platform}")
        archive = (source.parent if "file" in entry else state / "packages") / name
        paths[platform], _ = checked_archive(archive, entry["archiveSha256"])
    return identity, paths
