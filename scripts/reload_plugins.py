import json
import os
import selectors
import subprocess
import time


def verify_loaded(request, version):
    cursor = None
    identifier = 4
    while True:
        response = request(identifier, "mcpServerStatus/list", {"cursor": cursor, "limit": 100})
        for server in response["data"]:
            if server.get("pluginId") != "codex-session-board@session-board-rust":
                continue
            info = server.get("serverInfo") or {}
            if (info.get("name") != "codex-session-board" or info.get("version") != version
                    or server.get("toolsError") or "board_view" not in server["tools"]):
                raise RuntimeError("Installed board MCP version and tools were not confirmed")
            return {name: server.get(name) for name in
                    ("name", "pluginId", "serverInfo", "runtimeStatus", "toolsError")}
        cursor = response.get("nextCursor")
        if not cursor:
            raise RuntimeError("Installed board plugin was absent from Codex MCP inventory")
        identifier += 1


def reload_plugins(codex, directory, version):
    process = subprocess.Popen([codex, "app-server", "proxy"], cwd=directory,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    pending = bytearray()

    def send(message):
        process.stdin.write(json.dumps(message).encode() + b"\n")
        process.stdin.flush()

    def request(identifier, method, params):
        send({"id": identifier, "method": method, "params": params})
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if not selector.select(max(0, deadline - time.monotonic())):
                break
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError("Codex daemon proxy disconnected")
            pending.extend(chunk)
            while b"\n" in pending:
                line, _, remaining = pending.partition(b"\n")
                pending[:] = remaining
                message = json.loads(line)
                if message.get("id") != identifier:
                    continue
                if "error" in message:
                    raise RuntimeError(f"Codex rejected {method}, code {message['error'].get('code')}")
                return message["result"]
        raise TimeoutError(f"Codex did not finish {method}")

    try:
        request(1, "initialize", {"clientInfo": {"name": "session_board_update", "version": "1"},
                                 "capabilities": {"experimentalApi": True}})
        send({"method": "initialized"})
        request(2, "skills/list", {"cwds": [str(directory)], "forceReload": True})
        request(3, "config/mcpServer/reload", None)
        return verify_loaded(request, version)
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
