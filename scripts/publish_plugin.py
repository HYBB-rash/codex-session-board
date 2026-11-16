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


def prepare(repository, package, channel, manifest, checkout):
    reference = f"{channel}/{manifest['target']}"
    subprocess.run(["git", "init", "-q", str(checkout)], check=True)
    subprocess.run(["git", "remote", "add", "origin", repository], cwd=checkout, check=True)
    existing = command(["git", "ls-remote", "--heads", "origin", reference], checkout)
    previous_revision = existing.split()[0] if existing else ""
    if previous_revision:
        subprocess.run(["git", "fetch", "-q", "origin", previous_revision], cwd=checkout, check=True)
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
    return {"reference": reference, "revision": command(["git", "rev-parse", "HEAD"], checkout),
            "release": manifest, "previous_revision": previous_revision}


def remote_revisions(checkout, references):
    observed = command(["git", "ls-remote", "--heads", "origin", *references], checkout)
    return {reference: revision for revision, reference in (line.split() for line in observed.splitlines())}


def publish_many(repository, packages, channel):
    authorized = [(package, authorize(package, channel, evidence)) for package, evidence in packages]
    if not authorized or len({manifest["target"] for _, manifest in authorized}) != len(authorized):
        raise ValueError("Publish distinct platform packages together")
    identity_fields = ("source_commit", "build_sha256", "version", "contract_sha256")
    if len({tuple(manifest.get(field) for field in identity_fields) for _, manifest in authorized}) != 1:
        raise ValueError("Platform packages belong to different source, version or contract")
    with tempfile.TemporaryDirectory(prefix="board-publish-") as temporary:
        directories = [Path(temporary) / str(index) for index in range(len(authorized))]
        prepared = [prepare(repository, package, channel, manifest, checkout)
                    for (package, manifest), checkout in zip(authorized, directories)]
        checkout = directories[0]
        for publication, directory in zip(prepared[1:], directories[1:]):
            subprocess.run(["git", "fetch", "-q", str(directory), publication["revision"]],
                           cwd=checkout, check=True)
        expected = {f"refs/heads/{entry['reference']}": entry["previous_revision"] for entry in prepared}
        observed = remote_revisions(checkout, list(expected))
        if any(observed.get(reference, "") != revision for reference, revision in expected.items()):
            raise ValueError("Release channels changed while preparing publication; retry")
        leases = [f"--force-with-lease={reference}:{revision}" for reference, revision in expected.items()]
        updates = [f"{entry['revision']}:refs/heads/{entry['reference']}" for entry in prepared]
        subprocess.run(["git", "push", "--atomic", *leases, "origin", *updates], cwd=checkout, check=True)
        observed = remote_revisions(checkout, list(expected))
        if any(observed.get(f"refs/heads/{entry['reference']}") != entry["revision"] for entry in prepared):
            raise ValueError("Remote publication did not retain the expected platform revisions")
    return [{name: value for name, value in entry.items() if name != "previous_revision"} for entry in prepared]


def publish(repository, package, channel, evidence=None):
    return publish_many(repository, [(package, evidence)], channel)[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--package", type=Path, required=True, action="append")
    parser.add_argument("--channel", choices=("candidate", "stable"), default="candidate")
    parser.add_argument("--evidence", type=Path, action="append")
    parser.add_argument("--ssh-config", type=Path)
    arguments = parser.parse_args()
    if arguments.ssh_config:
        os.environ["GIT_SSH_COMMAND"] = shlex.join(["ssh", "-F", str(arguments.ssh_config.resolve())])
    if arguments.evidence and len(arguments.evidence) != len(arguments.package):
        parser.error("Supply one --evidence per --package")
    evidence = arguments.evidence or [None] * len(arguments.package)
    results = publish_many(arguments.repository, [
        (package.resolve(), record.resolve() if record else None)
        for package, record in zip(arguments.package, evidence)
    ], arguments.channel)
    result = results[0] if len(results) == 1 else {"publications": results}
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
