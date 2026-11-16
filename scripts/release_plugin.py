import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from urllib.parse import quote

from package_plugin import ROOT, TARGETS, build, command, verify_package, versions
from publish_plugin import authorize
from validate_context import require_remote_target, require_target
from verify_package_startup import CHECKS


def release_tag(source, reference="refs/heads/main"):
    branch = os.environ.get("GITHUB_REF_NAME") or command(["git", "branch", "--show-current"])
    if reference == "refs/heads/main":
        require_target(reference, "", branch)
        return f"v{versions(ROOT)}"
    configuration = json.loads(command(["git", "show", f"{source}:context/integration-target.json"]))
    git_command = lambda *arguments: command(["git", *arguments])
    require_target(reference, configuration["base"], branch, source, git_command=git_command)
    require_remote_target(reference, (configuration["base"], source), git_command=git_command)
    return f"rehearsal-{source}"


class GitHub:
    def __init__(self, repository):
        self.repository = repository
        self.release_ids = {}

    def gh(self, *arguments, check=True):
        return subprocess.run(
            ["gh", *arguments, "--repo", self.repository], check=check,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def api(self, path, method="GET", **fields):
        arguments = ["gh", "api", f"repos/{self.repository}/{path}", "--method", method]
        for name, value in fields.items():
            flag = "-F" if isinstance(value, (bool, int)) else "-f"
            if isinstance(value, bool):
                value = json.dumps(value)
            arguments.extend([flag, f"{name}={value}"])
        result = subprocess.run(arguments, text=True, capture_output=True)
        if result.returncode:
            if method == "GET" and "(HTTP 404)" in result.stderr:
                return None
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout or "null")

    def remember_release(self, tag, release):
        identifier = release.get("id") if isinstance(release, dict) else None
        if (type(identifier) is not int or identifier <= 0 or release.get("tag_name") != tag
                or identifier != self.release_ids.get(tag, identifier)):
            raise ValueError("GitHub did not confirm the requested release identity")
        self.release_ids[tag] = identifier
        return release

    def release(self, tag):
        if tag in self.release_ids:
            release = self.api(f"releases/{self.release_ids[tag]}")
            if release is None:
                raise ValueError(f"Confirmed release {self.release_ids[tag]} for {tag} is not readable")
            return self.remember_release(tag, release)
        release = self.api(f"releases/tags/{tag}")
        if release is not None:
            return self.remember_release(tag, release)
        page = 1
        while releases := self.api(f"releases?per_page=100&page={page}"):
            for release in releases:
                if release["tag_name"] == tag:
                    return self.remember_release(tag, release)
            page += 1
        return None

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
        release = self.release(tag)
        if release is None:
            try:
                release = self.api("releases", "POST", tag_name=tag, target_commitish=source,
                                   draft=True, prerelease=True, name=tag, body=f"Source commit: {source}")
            except RuntimeError:
                release = self.release(tag)
                if release is None:
                    raise
            else:
                if (not isinstance(release, dict) or release.get("tag_name") != tag
                        or release.get("draft") is not True):
                    raise ValueError("GitHub did not confirm the requested draft release")
        return self.remember_release(tag, release)

    def download(self, tag, name, destination):
        asset = next((entry for entry in self.release(tag)["assets"] if entry["name"] == name), None)
        if asset is None:
            raise ValueError(f"Release asset is missing: {name}")
        saved = destination / name
        with saved.open("wb") as output:
            subprocess.run(["gh", "api", f"repos/{self.repository}/releases/assets/{asset['id']}",
                            "-H", "Accept: application/octet-stream"],
                           check=True, stdout=output, stderr=subprocess.PIPE)
        return saved

    def upload(self, tag, path):
        upload_url = f"https://uploads.github.com/repos/{self.repository}/releases/{self.release(tag)['id']}/assets"
        result = subprocess.run(["gh", "api", f"{upload_url}?name={quote(path.name, safe='')}",
                                 "--method", "POST", "-H", "Content-Type: application/octet-stream",
                                 "--input", str(path)], text=True, capture_output=True)
        if result.returncode:
            release = self.release(tag)
            if not release or not any(asset["name"] == path.name for asset in release["assets"]):
                raise RuntimeError(result.stderr)

    def edit(self, tag, **fields):
        return self.api(f"releases/{self.release(tag)['id']}", "PATCH", **fields)


def asset_name(target):
    return f"plugin-{target}.tar.gz"


def startup_name(target):
    return f"STARTUP-{target}.json"


def validate_startup(report, manifest):
    if (not isinstance(report, dict) or report.get("schema_version") != 1
            or report.get("scope") != "package_startup"
            or report.get("passed") is not True or report.get("cleanup_confirmed") is not True
            or report.get("checks") != CHECKS):
        raise ValueError("Saved startup evidence is incomplete or failed")
    for field in ("source_commit", "build_sha256", "binary_sha256", "version", "target"):
        if not manifest.get(field) or report.get(field) != manifest[field]:
            raise ValueError(f"Saved startup evidence differs from package: {field}")


def read_startup(github, tag, manifest, directory):
    name = startup_name(manifest["target"])
    asset = next((entry for entry in github.release(tag)["assets"] if entry["name"] == name), None)
    if asset is None:
        return None
    saved = github.download(tag, name, directory)
    if asset.get("digest") != "sha256:" + hashlib.sha256(saved.read_bytes()).hexdigest():
        raise ValueError("GitHub startup asset digest is missing or differs")
    validate_startup(json.loads(saved.read_text()), manifest)
    return saved


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
    version = versions(ROOT) if tag == f"rehearsal-{source}" else tag[1:]
    if (manifest["source_commit"], manifest["version"], manifest["target"]) != (source, version, target):
        raise ValueError("Saved package belongs to different source, version or platform")
    return package


def save_platform(github, tag, target, source, directory, startup_report=None, fixture=None):
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
    manifest = authorize(package, "candidate", None)
    if startup_report is not None:
        saved = read_startup(github, tag, manifest, directory)
        if saved is None:
            if not github.release(tag)["draft"]:
                raise ValueError("Published release is missing startup evidence; use a new version")
            from verify_package_startup import verify as verify_startup
            verify_startup(package, fixture, startup_report)
            validate_startup(json.loads(startup_report.read_text()), manifest)
            saved = directory / startup_name(target)
            if saved.resolve() != startup_report.resolve():
                shutil.copyfile(startup_report, saved)
            github.upload(tag, saved)
            saved = read_startup(github, tag, manifest, directory)
            if saved is None:
                raise ValueError("Uploaded startup evidence could not be read back")
        else:
            output = startup_report.resolve()
            if output.exists() or output.is_relative_to(ROOT) or output.is_relative_to(package.resolve()):
                raise ValueError("Startup report must use a new path outside the source and package")
        if saved.resolve() != startup_report.resolve():
            shutil.copyfile(saved, startup_report)
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


def publish_saved(github, tag, source, channel, evidence, directory, reference="refs/heads/main"):
    rehearsal = reference != "refs/heads/main"
    if rehearsal and (channel != "candidate" or tag != release_tag(source, reference)):
        raise ValueError("Rehearsal releases require the exact source tag and candidate validation")
    if not rehearsal and tag.startswith("rehearsal-"):
        raise ValueError("Rehearsal releases cannot update delivery channels")
    if channel == "stable" and evidence is None:
        raise ValueError("Stable promotion requires reviewed evidence for both platforms")
    release = github.release(tag)
    if not isinstance(release, dict):
        raise ValueError("Release has not been saved")
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
        manifest = verify_package(package)
        startup = read_startup(github, tag, manifest, platform_directory)
        if startup is None and channel == "candidate":
            raise ValueError(f"Release is missing required startup evidence for {target}")
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
        try:
            github.edit(tag, draft=False, prerelease=True, make_latest="false")
        except RuntimeError:
            if github.release(tag)["draft"]:
                raise
    published = github.release(tag)
    if (not isinstance(published, dict) or published.get("immutable") is not True
            or published.get("draft") is not False):
        raise ValueError("Release must be immutable and published before updating channels")
    if rehearsal:
        return {"status": "published", "channel": None, "reference": reference,
                "tag": tag, "source": source}
    arguments = [sys.executable, str(checkout / "scripts/publish_plugin.py"),
                 "--repository", f"https://github.com/{github.repository}.git", "--channel", channel]
    arguments.extend(["--packages", *(str(package) for package, _ in packages)])
    for package, platform_evidence in packages:
        if platform_evidence:
            arguments.extend(["--evidence", str(platform_evidence)])
    subprocess.run(arguments, cwd=checkout, check=True)
    if channel == "stable":
        github.edit(tag, prerelease=False, make_latest="true")
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
    parser.add_argument("--reference", default="refs/heads/main")
    parser.add_argument("--target", choices=sorted(TARGETS))
    parser.add_argument("--finalize", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--save-only", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--startup-report", type=Path)
    parser.add_argument("--fixture", type=Path)
    arguments = parser.parse_args()
    if bool(arguments.startup_report) != bool(arguments.fixture) or (
            arguments.startup_report and not (arguments.channel == "candidate" and arguments.local)):
        parser.error("--startup-report and --fixture require a local candidate")
    repository = arguments.repository or json.loads(command(["gh", "repo", "view", "--json", "nameWithOwner"]))["nameWithOwner"]
    github = GitHub(repository)
    source = command(["git", "rev-parse", "HEAD"])
    tag = release_tag(source, arguments.reference)
    if arguments.reference != "refs/heads/main" and (
            arguments.channel != "candidate" or not (arguments.local or arguments.finalize)):
        parser.error("Rehearsal publication requires the explicit local or finalize candidate path")
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
                save_platform(github, tag, target, source, directory,
                              arguments.startup_report, arguments.fixture)
                if arguments.save_only:
                    print(json.dumps({"status": "saved", "tag": tag, "target": target}))
                    return
        elif github.release(tag) is None:
            raise ValueError("No saved candidate exists for this version")
        print(json.dumps(publish_saved(github, tag, source, arguments.channel, evidence,
                                       directory, arguments.reference)))


if __name__ == "__main__":
    main()
