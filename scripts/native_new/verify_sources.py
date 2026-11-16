import argparse
import json
from pathlib import Path
import subprocess

from prepare import files, patches, read_archive


def verify(source):
    if source.is_dir():
        sources = {"/" + str(path.relative_to(source)): path.read_bytes()
                   for path in source.rglob("*.js")}
        raw = bytearray()
        tree = {"files": {}}
        for name, data in sources.items():
            current = tree
            parts = name.strip("/").split("/")
            for part in parts[:-1]:
                current = current["files"].setdefault(part, {"files": {}})
            current["files"][parts[-1]] = {"offset": str(len(raw)), "size": len(data)}
            raw.extend(data)
        base = 0
    else:
        raw, base, tree = read_archive(source)
        sources = {name: raw[base + int(entry["offset"]):base + int(entry["offset"]) + entry["size"]]
                   for name, entry in files(tree) if "offset" in entry}
    changed = patches(tree, raw, base)
    original = {name: sources[name].decode() for name in changed}
    candidate = {name: data.decode() for name, data in changed.items()}
    subprocess.run(["node", str(Path(__file__).parent.parent / "check-native-target-source.mjs")],
                   input=json.dumps({"original": original, "candidate": candidate}),
                   text=True, check=True, timeout=60)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    verify(parser.parse_args().source)
