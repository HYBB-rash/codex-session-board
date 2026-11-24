#!/usr/bin/env python3
import argparse
from datetime import datetime, timezone
from hashlib import file_digest
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "state/acceptance/candidates"
PLATFORMS = ("linux-x86_64", "macos-arm64")


def build(ref, version, output, offline):
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[A-Za-z0-9_.-]+)?", version, re.ASCII):
        raise ValueError("版本格式无效")
    commit = subprocess.check_output(
        ["git", "rev-parse", "--verify", "--end-of-options", f"{ref}^{{commit}}"],
        cwd=ROOT, text=True,
    ).strip()
    output.mkdir(parents=True, exist_ok=True)
    destination = output / version
    if destination.exists() or destination.is_symlink():
        raise ValueError(f"候选目录已存在：{destination}")
    with tempfile.TemporaryDirectory(prefix=".candidate-", dir=output) as temporary:
        stage = Path(temporary)
        source, delivery = stage / "source", stage / "delivery"
        source.mkdir()
        delivery.mkdir()
        checkout = stage / "source.tar"
        subprocess.run(["git", "archive", "-o", str(checkout), commit], cwd=ROOT, check=True)
        subprocess.run(["tar", "-xf", str(checkout), "-C", str(source)], check=True)
        checks = {}
        log_path = delivery / "build.log"
        try:
            with log_path.open("w", encoding="utf-8") as log:
                def checked(name, *command):
                    print(f"board-candidate: {name}", file=sys.stderr, flush=True)
                    print(f"\n[{name}]", file=log, flush=True)
                    subprocess.run(command, cwd=source, stdout=log, stderr=subprocess.STDOUT, check=True)
                    checks[name] = "pass"

                checked("dependencies", "npm", "ci", "--no-audit", "--no-fund",
                        *(["--offline"] if offline else ["--prefer-offline"]))
                checked("lint", "npm", "run", "lint")
                checked("node-and-build", "npm", "test")
                checked("public-tests", "node", "--test", "acceptance/public-operations.test.mjs",
                        "acceptance/board-interaction.test.mjs")
                checked("frozen-model", sys.executable, "-B", "acceptance/verify-typescript.py", "--model-only")
                checked("release-gate-tests", sys.executable, "-B", "-m", "unittest", "discover",
                        "-s", "acceptance", "-p", "test_release_gate.py")
                archives = {}
                for platform in PLATFORMS:
                    checked(f"package:{platform}", "node", "dist/bin/codex-enhancer.js", "package",
                            version, str(delivery), platform)
                    archives[platform] = delivery / f"session-board-{version}-{platform}.tar.gz"
            command = [sys.executable, "-B", "acceptance/release_gate.py", "--identify"]
            for platform, archive in archives.items():
                command.extend(["--archive", f"{platform}={archive}"])
            identified = subprocess.run(command, cwd=source, capture_output=True, text=True, check=False)
            identity = json.loads(identified.stdout)
            if (identified.returncode or set(identity.get("archives", {})) != set(archives)
                    or any(issue for release in identity["archives"].values()
                           for issue in release["deliveryChecks"].values())):
                raise ValueError(f"候选归档检查失败：{identified.stdout or identified.stderr}")
        except (ValueError, subprocess.CalledProcessError) as error:
            if log_path.is_file():
                print("\n".join(log_path.read_text(encoding="utf-8").splitlines()[-65:]), file=sys.stderr)
            raise error
        with log_path.open("rb") as log:
            log_digest = file_digest(log, "sha256").hexdigest()
        manifest = {
            "schemaVersion": 1, "commit": commit, "version": version,
            "builtAt": datetime.now(timezone.utc).isoformat(),
            "contract": identity["contract"], "candidate": identity["candidate"],
            "archives": {platform: {**identity["archives"][platform], "file": archive.name}
                         for platform, archive in archives.items()},
            "checks": checks, "buildLog": log_path.name, "buildLogSha256": log_digest,
        }
        (delivery / "candidate.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        delivery.rename(destination)
    print(destination / "candidate.json")


def main():
    parser = argparse.ArgumentParser(description="从指定提交构建并核对双平台候选")
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("build")
    command.add_argument("--ref", required=True)
    command.add_argument("--version", required=True)
    command.add_argument("--output", type=Path, default=OUTPUT)
    command.add_argument("--offline", action="store_true", help="仅使用现有 npm 缓存")
    args = parser.parse_args()
    try:
        build(args.ref, args.version, args.output.resolve(), args.offline)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
