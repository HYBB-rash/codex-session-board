import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import socket
import stat
import sys
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.integrate_candidate import save
from scripts.shared_desktop import process_identity, socket_peer_pid


def now():
    return datetime.now(timezone.utc).isoformat()


class Rejected(RuntimeError):
    pass


class UnsupportedItems(Rejected):
    pass


class Rpc:
    def __init__(self, connection):
        self.connection = connection
        self.identifier = 0

    def request(self, method, params):
        deadline = min(time.monotonic() + 30, getattr(self, "deadline", float("inf")))
        if time.monotonic() >= deadline:
            raise TimeoutError(f"Request budget expired before {method}")
        self.connection.settimeout(max(0.001, deadline - time.monotonic()))
        self.identifier += 1
        self.connection.send(json.dumps({
            "id": self.identifier, "method": method, "params": params,
        }))
        while time.monotonic() < deadline:
            self.connection.settimeout(max(0.001, deadline - time.monotonic()))
            reply = json.loads(self.connection.recv())
            if reply.get("id") != self.identifier or "method" in reply:
                continue
            if "error" in reply:
                error = reply["error"]
                if (method == "thread/items/list" and error.get("code") == -32601
                        and error.get("message") == "thread/items/list is not supported yet"):
                    raise UnsupportedItems(f"{method}: {error}")
                raise Rejected(f"{method}: {reply['error']}")
            return reply["result"]
        raise TimeoutError(f"No receipt for {method}")


@contextmanager
def connect_endpoint(route):
    import websocket
    path = Path(route["socket"])
    metadata = path.stat()
    if (not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid()
            or metadata.st_mode & 0o077):
        raise ValueError("Author endpoint must be a private socket owned by this user")
    identity = route["backend"]
    with socket.socket(socket.AF_UNIX) as transport:
        transport.settimeout(30)
        transport.connect(str(path))
        if (socket_peer_pid(transport) != identity["pid"]
                or process_identity(identity["pid"]) != identity):
            raise ValueError("Author backend identity changed")
        connection = websocket.create_connection(
            "ws://localhost/rpc", socket=transport, timeout=30, suppress_origin=True)
