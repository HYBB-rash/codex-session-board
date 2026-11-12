import argparse
import hashlib
import json
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from package_plugin import ROOT, TARGETS, build, command, verify_package, versions
from publish_plugin import authorize


class GitHub:
    def __init__(self, repository):
        self.repository = repository

    def gh(self, *arguments, check=True):
        return subprocess.run(
            ["gh", *arguments, "--repo", self.repository], check=check,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def api(self, path, method="GET", **fields):
        arguments = ["gh", "api", f"repos/{self.repository}/{path}", "--method", method]
        for name, value in fields.items():
            arguments.extend(["-F", f"{name}={value}"])
        result = subprocess.run(arguments, text=True, capture_output=True)
        if result.returncode:
            if method == "GET" and "(HTTP 404)" in result.stderr:
                return None
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout or "null")

    def release(self, tag):
        return self.api(f"releases/tags/{tag}")

    def ensure_draft(self, tag, source):
        existing = self.api(f"git/ref/tags/{tag}")
        if existing is None:
            try:
                self.api("git/refs", "POST", ref=f"refs/tags/{tag}", sha=source)
            except RuntimeError:
                existing = self.api(f"git/ref/tags/{tag}")
                if existing is None:
                    raise
        existing = self.api(f"commits/{tag}")
        if existing["sha"] != source:
            raise ValueError("Version tag belongs to different source; use a new version")
        if self.release(tag) is None:
            result = self.gh("release", "create", tag, "--verify-tag", "--draft", "--prerelease",
                             "--title", tag, "--notes", f"Source commit: {source}", check=False)
            if result.returncode and self.release(tag) is None:
                raise RuntimeError(result.stderr)

    def download(self, tag, name, destination):
        self.gh("release", "download", tag, "--pattern", name, "--dir", str(destination))
        return destination / name

    def upload(self, tag, path):
        result = self.gh("release", "upload", tag, str(path), check=False)
        if result.returncode:
            release = self.release(tag)
            if not release or not any(asset["name"] == path.name for asset in release["assets"]):
                raise RuntimeError(result.stderr)


def asset_name(target):
    return f"plugin-{target}.tar.gz"


def unpack(archive, destination):
    destination.mkdir()
    with tarfile.open(archive) as contents:
        if any(not (entry.isfile() or entry.isdir()) for entry in contents.getmembers()):
            raise ValueError("Release archive contains links or special files")
        contents.extractall(destination, filter="data")
    return verify_package(destination)


def read_package(github, tag, target, source, directory):
    name = asset_name(target)
    release = github.release(tag)
    asset = next((entry for entry in release["assets"] if entry["name"] == name), None)
    if asset is None:
        return None
    archive = github.download(tag, name, directory)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if asset.get("digest") != f"sha256:{digest}":
        raise ValueError("GitHub asset digest is missing or does not match downloaded bytes")
    package = directory / "package"
    manifest = unpack(archive, package)
    if (manifest["source_commit"], manifest["version"], manifest["target"]) != (source, tag[1:], target):
        raise ValueError("Saved package belongs to different source, version or platform")
    return package


def save_platform(github, tag, target, source, directory):
    package = read_package(github, tag, target, source, directory)
    if package is None:
        if not github.release(tag)["draft"]:
            raise ValueError("Published release is missing a platform; use a new version")
        built = build(ROOT, target, directory / "built")
        archive = directory / asset_name(target)
        with tarfile.open(archive, "w:gz") as contents:
            for path in sorted(built.iterdir()):
                contents.add(path, arcname=path.name)
        github.upload(tag, archive)
        archive.unlink()
        package = read_package(github, tag, target, source, directory)
    if package is None:
        raise ValueError("Uploaded platform package could not be read back")
    authorize(package, "candidate", None)
    archive = directory / asset_name(target)
    checksum = archive.with_name(archive.name + ".sha256")
    checksum.write_text(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
    if checksum.name not in {asset["name"] for asset in github.release(tag)["assets"]}:
        github.upload(tag, checksum)
    with tempfile.TemporaryDirectory() as temporary:
        saved = github.download(tag, checksum.name, Path(temporary))
        if saved.read_bytes() != checksum.read_bytes():
            raise ValueError("Saved checksum differs from canonical package")


def source_checkout(source, directory):
    command(["git", "clone", "--quiet", "--shared", "--no-checkout", str(ROOT), str(directory)])
    command(["git", "checkout", "--quiet", "--detach", source], directory)


def publish_saved(github, tag, source, channel, evidence, directory):
    if channel == "stable" and evidence is None:
        raise ValueError("Stable promotion requires reviewed evidence for both platforms")
    release = github.release(tag)
    if github.api(f"commits/{tag}")["sha"] != source:
        raise ValueError("Version tag belongs to different source; use a new version")
    required = {asset_name(target) + suffix for target in TARGETS for suffix in ("", ".sha256")}
    if not required.issubset({entry["name"] for entry in release["assets"]}):
        if channel == "stable" or not release["draft"]:
            raise ValueError("Release is missing required platform packages or checksums")
        return {"status": "incomplete", "tag": tag, "message": "Draft retained; waiting for both platforms"}
    checkout = directory / "source"
    source_checkout(source, checkout)
    packages = []
    for target in sorted(TARGETS):
        platform_directory = directory / target
        platform_directory.mkdir()
        package = read_package(github, tag, target, source, platform_directory)
        checksum = github.download(tag, asset_name(target) + ".sha256", platform_directory)
        archive = platform_directory / asset_name(target)
        expected = f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n"
        if checksum.read_text() != expected:
            raise ValueError("Release checksum mismatch")
        platform_evidence = evidence_for(evidence, target)
        validation = "from pathlib import Path; from publish_plugin import authorize; import sys; " \
                     "authorize(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]) if sys.argv[3] else None)"
        subprocess.run([sys.executable, "-c", validation, str(package), channel,
                        str(platform_evidence) if platform_evidence else ""], cwd=checkout / "scripts", check=True)
        packages.append((package, platform_evidence))
    if release["draft"]:
        if channel == "stable":
            raise ValueError("Publish and validate a candidate before promoting stable")
        try:
            settings = github.api("immutable-releases")
        except RuntimeError as error:
            if "(HTTP 403)" not in str(error):
                raise
            settings = None
        if settings is not None and not settings["enabled"]:
            raise ValueError("Enable GitHub immutable releases before publishing")
        result = github.gh("release", "edit", tag, "--draft=false", "--prerelease", "--latest=false", check=False)
        if result.returncode and github.release(tag)["draft"]:
            raise RuntimeError(result.stderr)
    if not github.release(tag).get("immutable"):
        raise ValueError("Release must be immutable before updating channels")
    for package, platform_evidence in packages:
        arguments = [sys.executable, str(checkout / "scripts/publish_plugin.py"),
                     "--repository", f"https://github.com/{github.repository}.git",
                     "--package", str(package), "--channel", channel]
        if platform_evidence:
            arguments.extend(["--evidence", str(platform_evidence)])
        subprocess.run(arguments, cwd=checkout, check=True)
    if channel == "stable":
        github.gh("release", "edit", tag, "--prerelease=false", "--latest")
    return {"status": "published", "channel": channel, "tag": tag, "source": source}


def evidence_for(index, target):
    if index is None:
        return None
    records = json.loads(index.read_text())["platforms"]
    return (index.parent / records[target]).resolve(strict=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("channel", choices=["candidate", "stable"])
    parser.add_argument("--local", action="store_true")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--target", choices=sorted(TARGETS))
    parser.add_argument("--finalize", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--save-only", action="store_true", help=argparse.SUPPRESS)
    arguments = parser.parse_args()
    repository = arguments.repository or json.loads(command(["gh", "repo", "view", "--json", "nameWithOwner"]))["nameWithOwner"]
    github = GitHub(repository)
    source = command(["git", "rev-parse", "HEAD"])
    tag = f"v{versions(ROOT)}"
    if command(["git", "status", "--porcelain", "--untracked-files=all"]):
        raise ValueError("Release requires a clean committed source tree")
    evidence = arguments.evidence.resolve() if arguments.evidence else None
    if arguments.channel == "stable" and evidence is None:
        parser.error("stable requires --evidence")
    if arguments.channel == "candidate" and not arguments.local and not arguments.finalize:
        github.ensure_draft(tag, source)
        github.gh("workflow", "run", "plugin-candidate.yml", "--ref", tag)
        print(json.dumps({"status": "dispatched", "source": source, "tag": tag}))
        return
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        if arguments.channel == "candidate":
            github.ensure_draft(tag, source)
            if arguments.local:
                target = arguments.target or command(["rustc", "-vV"]).split("host: ")[1].splitlines()[0]
                if target not in TARGETS:
                    raise ValueError(f"Unsupported native platform: {target}")
                save_platform(github, tag, target, source, directory)
                if arguments.save_only:
                    print(json.dumps({"status": "saved", "tag": tag, "target": target}))
                    return
        elif github.release(tag) is None:
            raise ValueError("No saved candidate exists for this version")
        print(json.dumps(publish_saved(github, tag, source, arguments.channel, evidence, directory)))


if __name__ == "__main__":
    main()
