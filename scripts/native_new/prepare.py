import argparse
import copy
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile


SUPPORTED_SOURCES = {
    "/.vite/build/early-bootstrap-615-24f1d3fc72897a7a.js":
        "b86b7e90fb910eac2937058bb6787222221a933c158ad622a2e9b26b03349e60",
    "/.vite/build/early-bootstrap-631-9880b56d55f301d1.js":
        "bce29a615bb1ab7019b9db27acdff7018ddaae8a2c7d6ede4b6c983b01f90ebe",
    "/.vite/build/early-bootstrap-226-2f6bd181558008cf.js":
        "97adba36c0c0e9e145ab364c82eb3433abe3675ab7c6e3ab282035b67a4d60bb",
    "/.vite/build/early-bootstrap-631-c364c4eef36b8fb5.js":
        "3d33823eff6f760caed93a06c7bd580a2ed7631c961bf7f4454622c47205fc2c",
}
BINDINGS = {
    "/.vite/build/early-bootstrap-615-24f1d3fc72897a7a.js": ("eX", "es", True),
    "/.vite/build/early-bootstrap-226-2f6bd181558008cf.js": ("tw", "e0", False),
}


def files(tree, prefix=""):
    for name, entry in tree["files"].items():
        path = f"{prefix}/{name}"
        if "files" in entry:
            yield from files(entry, path)
        else:
            yield path, entry


def read_archive(path):
    raw = path.read_bytes()
    _, header_size, _, json_size = struct.unpack("<4I", raw[:16])
    return raw, 8 + header_size, json.loads(raw[16:16 + json_size])


def replace_once(source, anchor, replacement):
    if source.count(anchor) != 1:
        raise ValueError(f"Unsupported native client: expected one {anchor}")
    return source.replace(anchor, replacement)


def dispatch(opener, binding):
    projects, state, separate_host = binding
    selection = f"project=>(0,{projects}.NR)(r,project)"
    keys = [f"{state}.Q2Y.SELECTED_PROJECT"]
    if separate_host:
        selection = (f"async project=>{{await (0,{projects}.NR)(r,project);"
                     f"r.set({state}.Q2Y.SELECTED_REMOTE_HOST_ID,t.hostId)}}")
        keys.append(f"{state}.Q2Y.SELECTED_REMOTE_HOST_ID")
    return ('case"remoteNewThread":{await(' + opener + ')({route:t,'
            'projectsManager:a,selectProject:' + selection + ','
            'announce:()=>o.sendMessageToAllWindows({type:"global-state-updated",'
            'keys:[' + ",".join(keys) + ']}),navigate:project=>l(e,"/",'
            '{project,focusComposerNonce:Date.now()})});return}')


def patches(tree, raw, base):
    root = Path(__file__).parent
    parser = (root / "route.js").read_text().strip()
    opener = (root / "open.js").read_text().strip()
    result = {}
    for name, entry in files(tree):
        if name not in SUPPORTED_SOURCES or "offset" not in entry:
            continue
        start = base + int(entry["offset"])
        data = raw[start:start + entry["size"]]
        if hashlib.sha256(data).hexdigest() != SUPPORTED_SOURCES[name]:
            raise ValueError(f"Unsupported native client source: {name}")
        source = data.decode()
        if 'case"threads":{let e=L(t);' in source:
            source = replace_once(source, 'case"threads":{let e=L(t);',
                'case"threads":{let e=L(t);if("new-remote"===e[0])return(' + parser + ')(t);')
            result[name] = source.encode()
        if 'case"newThread":{let s=' in source:
            if name not in BINDINGS:
                raise ValueError(f"Unsupported native dispatcher: {name}")
            branch = dispatch(opener, BINDINGS[name])
            source = replace_once(source, 'case"newThread":{let s=',
                branch + 'case"newThread":{let s=')
            result[name] = source.encode()
    if len(result) != 2:
        raise ValueError("Unsupported native client: missing remote draft integration points")
    for source in result.values():
        with tempfile.NamedTemporaryFile(suffix=".js") as script:
            script.write(source)
            script.flush()
            subprocess.run(["node", "--check", script.name], check=True, capture_output=True)
    return result


def encode_archive(raw, base, original, changes):
    tree = copy.deepcopy(original)
    payload = bytearray()
    for name, entry in files(tree):
        if "offset" not in entry:
            continue
        offset = base + int(entry["offset"])
        data = changes.get(name, raw[offset:offset + entry["size"]])
        entry["offset"] = str(len(payload))
        entry["size"] = len(data)
        if name in changes:
            block_size = entry.get("integrity", {}).get("blockSize", 4194304)
            entry["integrity"] = {
                "algorithm": "SHA256", "hash": hashlib.sha256(data).hexdigest(),
                "blockSize": block_size, "blocks": [
                    hashlib.sha256(data[start:start + block_size]).hexdigest()
                    for start in range(0, len(data), block_size)
                ],
            }
        payload.extend(data)
    header = json.dumps(tree, separators=(",", ":"), ensure_ascii=False).encode()
    padding = b"\0" * (-(len(header) + 4) % 4)
    pickle_size = len(header) + len(padding) + 4
    return struct.pack("<4I", 4, pickle_size + 4, pickle_size, len(header)) + header + padding + payload


def launch_archive(command):
    if command[0] in ["open", "/usr/bin/open"]:
        if "-a" not in command or command.index("-a") + 1 >= len(command):
            raise ValueError("The native open command must select an explicit app")
        app = Path(command[command.index("-a") + 1])
        if not app.is_absolute() or app.suffix != ".app":
            raise ValueError("The native app path must be absolute")
        return app / "Contents/Resources/app.asar"
    executable = Path(command[0])
    if not executable.is_absolute():
        raise ValueError("The native executable path must be absolute")
    if executable.parent.name == "MacOS":
        return executable.parent.parent / "Resources/app.asar"
    return executable.parent / "resources/app.asar"


def prepare(source, target, command):
    capability_path = target.with_suffix(".remote-new.json")
    if source.resolve() == target.resolve() or any(
        path.exists() or path.is_symlink() for path in [target, capability_path]
    ):
        raise ValueError("The native candidate must be a new archive")
    if not command or not all(isinstance(part, str) and part for part in command):
        raise ValueError("An explicit native candidate open command is required")
    if launch_archive(command).resolve() != target.resolve():
        raise ValueError("The native command does not launch the candidate archive")
    raw, base, tree = read_archive(source)
    changed = patches(tree, raw, base)
    candidate = encode_archive(raw, base, tree, changed)
    with target.open("xb") as output:
        output.write(candidate)
    capability = {
        "protocol": "codex-remote-draft-v1", "archive": str(target.resolve()),
        "sha256": hashlib.sha256(candidate).hexdigest(), "openCommand": command,
        "sourceSha256": hashlib.sha256(raw).hexdigest(),
        "patchedFiles": sorted(changed),
    }
    with capability_path.open("x") as output:
        output.write(json.dumps(capability, ensure_ascii=False, indent=2) + "\n")
    return capability


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("target", type=Path)
    parser.add_argument("--open-command", required=True, type=json.loads)
    arguments = parser.parse_args()
    print(json.dumps(prepare(arguments.source, arguments.target, arguments.open_command), indent=2))
