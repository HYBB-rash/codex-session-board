import json
import os
import struct
import subprocess
import sys
import threading

LIMIT = 64 * 1024 * 1024


def compact(payload):
    message = json.loads(payload)
    result = message.get("result")
    turns = result.get("data") if isinstance(result, dict) else None
    if not isinstance(turns, list) or not turns or not all(
        isinstance(turn, dict) and isinstance(turn.get("items"), list)
        and isinstance(turn.get("id"), str) and "status" in turn for turn in turns
    ):
        return payload
    for turn in turns:
        turn["items"] = [
            {key: item[key] for key in ("id", "type", "phase") if key in item}
            for item in turn["items"]
            if isinstance(item, dict) and item.get("type") in ("userMessage", "agentMessage")
        ]
    return json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode()


def exact(stream, length):
    value = stream.read(length)
    if len(value) != length:
        raise EOFError("proxy closed")
    return value


def frame(output, opcode, payload):
    length = len(payload)
    size = bytes([length]) if length < 126 else (
        b"\x7e" + struct.pack("!H", length) if length <= 65535
        else b"\x7f" + struct.pack("!Q", length)
    )
    output.write(bytes([0x80 | opcode]) + size + payload)
    output.flush()


def relay(source, output):
    header = bytearray()
    while not header.endswith(b"\r\n\r\n"):
        if len(header) >= 65536:
            raise ValueError("proxy HTTP header too large")
        header.extend(exact(source, 1))
    output.write(header)
    output.flush()
    fragments, kind = bytearray(), None
    while True:
        first, second = exact(source, 2)
        if first & 0x70 or second & 0x80:
            raise ValueError("unexpected proxy frame flags")
        length = second & 127
        if length == 126:
            length = struct.unpack("!H", exact(source, 2))[0]
        elif length == 127:
            length = struct.unpack("!Q", exact(source, 8))[0]
        opcode = first & 15
        if length + len(fragments) > LIMIT:
            raise ValueError("proxy message too large")
        payload = exact(source, length)
        if opcode >= 8:
            frame(output, opcode, payload)
            if opcode == 8:
                return
            continue
        if opcode in (1, 2) and kind is None:
            kind = opcode
        elif opcode != 0 or kind is None:
            raise ValueError("unexpected proxy continuation")
        fragments.extend(payload)
        if first & 0x80:
            frame(output, kind, compact(fragments) if kind == 1 else fragments)
            fragments, kind = bytearray(), None


def upstream(process):
    try:
        while True:
            data = os.read(0, 65536)
            if not data:
                break
            process.stdin.write(data)
            process.stdin.flush()
    except BrokenPipeError:
        pass
    finally:
        process.stdin.close()


def main():
    process = subprocess.Popen(
        ["/bin/sh", "-c", "exec codex app-server proxy"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE
    )
    try:
        threading.Thread(target=upstream, args=(process,), daemon=True).start()
        relay(process.stdout, sys.stdout.buffer)
    except (EOFError, BrokenPipeError):
        pass
    finally:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


if __name__ == "__main__":
    main()
