import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import subprocess
import sys

from prepare import prepare, read_archive


def header_digest(archive):
    raw = archive.read_bytes()
    _, _, tree = read_archive(archive)
    if not tree.get("files"):
        raise ValueError("The native archive has no files")
    length = int.from_bytes(raw[12:16], "little")
    return hashlib.sha256(raw[16:16 + length]).hexdigest()


def prepare_app(source, target):
    if sys.platform != "darwin":
        raise ValueError("Build macOS native candidates on macOS")
    source, target = source.resolve(), target.absolute()
    if source.suffix != ".app" or target.suffix != ".app" or not source.is_dir():
        raise ValueError("Source and target must be macOS app bundles")
    if target.exists() or target.is_symlink() or target.resolve().is_relative_to(source):
        raise ValueError("The candidate must be a new app outside the source bundle")
    archive = source / "Contents/Resources/app.asar"
    original = plistlib.loads((source / "Contents/Info.plist").read_bytes())
    integrity = original["ElectronAsarIntegrity"]["Resources/app.asar"]
    if integrity != {"algorithm": "SHA256", "hash": header_digest(archive)}:
        raise ValueError("The source app archive integrity could not be verified")
    target.mkdir(parents=True)
    subprocess.run(["/bin/cp", "-cR", str(source) + "/.", str(target)], check=True)
    candidate = target / "Contents/Resources/app.asar"
    candidate.unlink()
    capability = prepare(archive, candidate, ["open", "-a", str(target)])
    original["ElectronAsarIntegrity"]["Resources/app.asar"]["hash"] = header_digest(candidate)
    (target / "Contents/Info.plist").write_bytes(plistlib.dumps(original))
    subprocess.run(["/usr/bin/codesign", "--force", "--deep", "--sign", "-",
                    "--preserve-metadata=entitlements,flags,runtime", str(target)], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(target)], check=True)
    return capability


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("target", type=Path)
    arguments = parser.parse_args()
    print(json.dumps(prepare_app(arguments.source, arguments.target), indent=2))
