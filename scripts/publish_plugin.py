import argparse
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import tempfile
from pathlib import Path

from package_plugin import ROOT, command, fingerprint, verify_package, versions
from verify_remote_release import verify


def authorize(package, channel, evidence, root=ROOT):
    manifest = verify_package(package)
    bindings = json.loads((root / "acceptance/bindings.json").read_text())
    if manifest["source_commit"] != command(["git", "rev-parse", "HEAD"], root):
        raise ValueError("Package does not belong to the current source commit")
    if manifest["build_sha256"] != fingerprint(root, bindings["fingerprint_paths"]):
        raise ValueError("Package source fingerprint is stale")
    if (manifest["version"] != versions(root)
            or manifest["contract_sha256"] != hashlib.sha256((root / "acceptance/requirements.json").read_bytes()).hexdigest()):
        raise ValueError("Package version or acceptance contract differs")
    if command(["git", "status", "--porcelain"], root):
        raise ValueError("Publish only from a clean checkout")
    if channel == "stable":
        if evidence is None:
            raise ValueError("Stable publication requires reviewed full native/remote evidence")
        report, status = verify(root, evidence, package / "bin/codex-session-board")
        if status or report["release_ready"] is not True:
            raise ValueError("Full release acceptance has not passed")
    return manifest


def validate_channel_progression(previous, manifest):
    if tuple(map(int, previous["version"].split("."))) > tuple(map(int, manifest["version"].split("."))):
        raise ValueError("Cannot downgrade a release channel; use a pinned consumer rollback")
    if previous["version"] == manifest["version"] and previous != manifest:
        raise ValueError("Published versions are immutable; changed artifacts require a new version")


def publish(repository, package, channel, evidence=None):
    manifest = authorize(package, channel, evidence)
    reference = f"{channel}/{manifest['target']}"
    with tempfile.TemporaryDirectory(prefix="board-publish-") as temporary:
        checkout = Path(temporary)
        subprocess.run(["git", "init", "-q", str(checkout)], check=True)
        subprocess.run(["git", "remote", "add", "origin", repository], cwd=checkout, check=True)
        existing = command(["git", "ls-remote", "--heads", "origin", reference], checkout)
        if existing:
            subprocess.run(["git", "fetch", "-q", "origin", reference], cwd=checkout, check=True)
            subprocess.run(["git", "checkout", "-q", "--detach", "FETCH_HEAD"], cwd=checkout, check=True)
            previous = verify_package(checkout)
            validate_channel_progression(previous, manifest)
            for path in checkout.iterdir():
                if path.name != ".git":
                    shutil.rmtree(path) if path.is_dir() else path.unlink()
        shutil.copytree(package, checkout, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git"))
        verify_package(checkout)
        subprocess.run(["git", "add", "."], cwd=checkout, check=True)
        signer = shutil.which("agent-git") or "git"
        if subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=checkout).returncode:
            subprocess.run([signer, "commit", "-qm",
                            f"Publish {manifest['version']} for {manifest['target']} ({channel})"],
                           cwd=checkout, check=True)
        revision = command(["git", "rev-parse", "HEAD"], checkout)
        subprocess.run(["git", "push", "origin", f"HEAD:refs/heads/{reference}"], cwd=checkout, check=True)
        observed = command(["git", "ls-remote", "origin", f"refs/heads/{reference}"], checkout)
        if observed.split()[0] != revision:
            raise ValueError("Remote publication did not retain the expected revision")
    return {"reference": reference, "revision": revision, "release": manifest}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--channel", choices=("candidate", "stable"), default="candidate")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--ssh-config", type=Path)
    arguments = parser.parse_args()
    if arguments.ssh_config:
        os.environ["GIT_SSH_COMMAND"] = shlex.join(["ssh", "-F", str(arguments.ssh_config.resolve())])
    result = publish(arguments.repository, arguments.package.resolve(), arguments.channel,
                     arguments.evidence.resolve() if arguments.evidence else None)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
