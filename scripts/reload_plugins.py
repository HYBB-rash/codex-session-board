import json
import os
import platform
import socket
import stat
import time
from pathlib import Path


def verify_loaded(request, version, thread_id=None):
    cursor = None
    identifier = 4
    while True:
        params = {"cursor": cursor, "limit": 100, "serverName": "session_board"}
        if thread_id:
            params["threadId"] = thread_id
        response = request(identifier, "mcpServerStatus/list", params)
        for server in response["data"]:
            if server.get("pluginId") != "codex-session-board@session-board-rust":
                continue
            info = server.get("serverInfo") or {}
            if (info.get("name") != "codex-session-board" or info.get("version") != version
                    or server.get("toolsError") or "board_view" not in (server.get("tools") or {})):
                raise RuntimeError("Installed board MCP version and tools were not confirmed")
            if thread_id:
                snapshot = request(identifier + 1, "mcpServer/tool/call", {
                    "threadId": thread_id, "server": "session_board", "tool": "board_snapshot", "arguments": {}})
                content = snapshot.get("structuredContent")
                if snapshot.get("isError") or not isinstance(content, dict) or not isinstance(content.get("sessions"), list):
                    raise RuntimeError("Current thread board snapshot was not confirmed")
            return {name: server.get(name) for name in
                    ("name", "pluginId", "serverInfo", "runtimeStatus", "toolsError")}
        cursor = response.get("nextCursor")
        if not cursor:
            raise RuntimeError("Installed board plugin was absent from Codex MCP inventory")
        identifier += 1


def desktop_socket():
    configured = os.environ.get("CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET")
    if configured:
        path = Path(configured)
    elif platform.system() == "Linux":
        runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
        path = runtime / "codex-desktop/app-server-bridge/app-server.sock"
    else:
        return None
    try:
        metadata = path.stat()
    except FileNotFoundError:
        return None
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise RuntimeError("Codex desktop socket must be private and owned by this user")
    return path


def reload_plugins(codex, directory, version, marketplace_path=None):
    path = desktop_socket()
    thread_id = os.environ.get("CODEX_THREAD_ID", "").strip()
    if path is None or not thread_id:
        return {"phase": "awaiting_desktop_reload", "desktop_loaded": False}
    marketplace = Path(marketplace_path) if marketplace_path is not None else None
    if marketplace is not None and (not marketplace.is_file() or marketplace.name != "marketplace.json"):
        raise ValueError("Desktop plugin install requires a marketplace.json file")
    import websocket
    with socket.socket(socket.AF_UNIX) as transport:
        transport.settimeout(30)
        transport.connect(str(path))
        connection = websocket.create_connection("ws://localhost/rpc", socket=transport,
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
            thread = request(0, "thread/read", {"threadId": thread_id, "includeTurns": False})
            if thread.get("thread", {}).get("status", {}).get("type") not in {"idle", "active", "systemError"}:
                raise RuntimeError("Current thread is not loaded in the selected desktop; reload was not attempted")
            if marketplace is not None:
                request(2, "plugin/install", {"marketplacePath": str(marketplace),
                                              "pluginName": "codex-session-board"})
            request(3, "config/mcpServer/reload", None)
            return {"phase": "desktop_loaded", "desktop_loaded": True,
                    "socket": str(path), "threadId": thread_id, "mcp": verify_loaded(request, version, thread_id)}
        finally:
            connection.close()
