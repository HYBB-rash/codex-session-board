import json
import os
import socket
import stat
import subprocess
import time
from pathlib import Path


def verify_loaded(request, version):
    cursor = None
    identifier = 4
    while True:
        response = request(identifier, "mcpServerStatus/list", {
            "cursor": cursor, "limit": 100, "serverName": "session_board"})
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
    import websocket
    daemon = json.loads(subprocess.check_output(
        [codex, "app-server", "daemon", "version"], cwd=directory, text=True, timeout=10))
    path = Path(daemon["socketPath"]).resolve(strict=True)
    metadata = path.stat()
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise RuntimeError("Codex daemon socket must be private and owned by this user")
    with socket.socket(socket.AF_UNIX) as transport:
        transport.settimeout(30)
        transport.connect(str(path))
        connection = websocket.create_connection("ws://localhost/", socket=transport,
                                                 timeout=30, suppress_origin=True)
        def request(identifier, method, params):
            connection.send(json.dumps({"id": identifier, "method": method, "params": params}))
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                connection.settimeout(max(0.001, deadline - time.monotonic()))
                message = json.loads(connection.recv())
                if message.get("method") or message.get("id") != identifier:
                    continue
                if "error" in message:
                    raise RuntimeError(f"Codex rejected {method}, code {message['error'].get('code')}")
                return message["result"]
            raise TimeoutError(f"Codex did not finish {method}")
        try:
            request(1, "initialize", {"clientInfo": {"name": "session_board_update", "version": "1"},
                                     "capabilities": {"experimentalApi": True}})
            connection.send(json.dumps({"method": "initialized"}))
            request(3, "config/mcpServer/reload", None)
            return verify_loaded(request, version)
        finally:
            connection.close()
