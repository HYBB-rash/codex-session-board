import hashlib
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys


def process_identity(pid):
    if sys.platform == "darwin":
        output = subprocess.check_output(["/bin/ps", "-p", str(pid), "-o", "stat=", "-o", "lstart="],
                                         env={**os.environ, "LC_ALL": "C"}, text=True, timeout=3).split()
        if not output or output[0].startswith("Z"):
            raise RuntimeError("共享客户端进程已退出")
        started = int.from_bytes(hashlib.sha256(" ".join(output[1:]).encode()).digest()[:8], "big")
    else:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] in ("Z", "X"):
            raise RuntimeError("共享客户端进程已退出")
        started = int(fields[19])
    return {"pid": pid, "started": started}


def socket_peer_pid(connection):
    if sys.platform == "darwin":
        return struct.unpack("i", connection.getsockopt(0, 2, 4))[0]
    return struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[0]
