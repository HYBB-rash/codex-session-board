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
        self.identifier += 1
        self.connection.send(json.dumps({
            "id": self.identifier, "method": method, "params": params,
        }))
        deadline = time.monotonic() + 30
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
        try:
            rpc = Rpc(connection)
            rpc.request("initialize", {
                "clientInfo": {"name": "pr_queue", "version": "1"},
                "capabilities": {"experimentalApi": True},
            })
            connection.send(json.dumps({"method": "initialized"}))
            yield rpc
        finally:
            connection.close()


@contextmanager
def connect(route):
    with connect_endpoint(route) as rpc:
        current = rpc.request("thread/read", {
            "threadId": route["coordinator_thread"], "includeTurns": False,
        })["thread"]
        if (current["id"] != route["coordinator_thread"]
                or current["status"]["type"] not in {"idle", "active"}):
            raise ValueError("Coordinator is not loaded in this backend")
        yield rpc


def read_author(rpc, route, check_cwd=True):
    thread = rpc.request("thread/read", {
        "threadId": route["thread_id"], "includeTurns": False,
    })["thread"]
    if (thread["id"] != route["thread_id"]
            or check_cwd and thread["cwd"] not in {route["expected_cwd"], route["repair_cwd"]}):
        raise ValueError("Author thread or working directory does not match its route")
    return thread


def acknowledge(receipt, item, timestamp):
    if (item["type"] == "agentMessage"
            and f"PR_QUEUE_ACK {receipt['request_id']}" in item["text"].splitlines()):
        if "acknowledged_at" not in receipt:
            receipt["acknowledged_at"] = datetime.fromtimestamp(
                timestamp / 1000, timezone.utc).isoformat() if timestamp is not None else now()
            receipt["ack_time_basis"] = (
                "item_completed_at" if timestamp is not None else "observed_upper_bound")
        receipt["ack_item_id"] = item["id"]
        receipt["state"] = "acknowledged"


def observe_turn(receipt, turn):
    receipt["turn_status"] = turn["status"]
    if turn["status"] == "completed" and receipt.get("acknowledged_at"):
        receipt["state"] = "author_turn_completed"
        timestamp = turn.get("completedAt")
        receipt.setdefault("completed_at", datetime.fromtimestamp(
            timestamp, timezone.utc).isoformat() if timestamp else now())
    elif turn["status"] in {"failed", "interrupted"}:
        receipt.update(state="author_failed", author_error=turn.get("error"))


def reconcile_turn(receipt, matches):
    if len(matches) > 1:
        raise ValueError("A feedback ID appears in multiple turns")
    if matches:
        receipt.update(turn_id=matches.pop(), state="sent", reconciled_at=now())


def observe_thread(rpc, route, receipt):
    thread = rpc.request("thread/read", {
        "threadId": route["thread_id"], "includeTurns": True,
    })["thread"]
    if thread["id"] != route["thread_id"]:
        raise ValueError("Author thread does not match its route")
    if not receipt.get("turn_id"):
        matches = {turn["id"] for turn in thread["turns"] for item in turn["items"]
                   if item["type"] == "userMessage"
                   and item.get("clientId") == receipt["request_id"]}
        reconcile_turn(receipt, matches)
        if not receipt.get("turn_id"):
            return
    turns = [turn for turn in thread["turns"] if turn["id"] == receipt["turn_id"]]
    if len(turns) != 1:
        raise ValueError("Delivered turn is missing or duplicated in the original thread")
    turn = turns[0]
    for item in turn["items"]:
        acknowledge(receipt, item, item.get("completedAtMs"))
    observe_turn(receipt, turn)
    receipt["observation_method"] = "thread/read"


def observe(rpc, route, receipt):
    observation = receipt.copy()
    try:
        observe_items(rpc, route, observation)
    except UnsupportedItems:
        observation = receipt.copy()
        observe_thread(rpc, route, observation)
    receipt.update(observation)


def observe_items(rpc, route, receipt):
    if not receipt.get("turn_id"):
        if receipt["state"] != "delivery_unknown":
            return
        cursor, matches = None, set()
        while True:
            page = rpc.request("thread/items/list", {
                "threadId": route["thread_id"], "limit": 100, "sortDirection": "desc",
                "cursor": cursor, "deferOutput": {"commandExecution": True, "mcpToolCall": True},
            })
            matches.update(entry["turnId"] for entry in page["data"]
                           if entry["item"]["type"] == "userMessage"
                           and entry["item"].get("clientId") == receipt["request_id"])
            cursor = page.get("nextCursor")
            if not cursor:
                break
        reconcile_turn(receipt, matches)
        if not receipt.get("turn_id"):
            return
    cursor = None
    while True:
        page = rpc.request("thread/items/list", {
            "threadId": route["thread_id"], "turnId": receipt["turn_id"],
            "limit": 100, "cursor": cursor,
        })
        for entry in page["data"]:
            if entry["turnId"] != receipt["turn_id"]:
                raise ValueError("Item does not belong to the delivered turn")
            acknowledge(receipt, entry["item"], entry.get("completedAtMs"))
        cursor = page.get("nextCursor")
        if not cursor:
            break
    cursor = None
    while True:
        page = rpc.request("thread/turns/list", {
            "threadId": route["thread_id"], "limit": 20, "cursor": cursor,
            "itemsView": "summary",
        })
        found = next((turn for turn in page["data"] if turn["id"] == receipt["turn_id"]), None)
        if found:
            observe_turn(receipt, found)
            break
        cursor = page.get("nextCursor")
        if not cursor:
            raise ValueError("Delivered turn is missing from the original thread")


def send(rpc, route, request, receipt, output, prompt=None):
    if receipt["state"] not in {"pending", "waiting_idle", "rejected"}:
        observe(rpc, route, receipt)
        return
    if not Path(route["repair_cwd"]).is_dir():
        raise ValueError("An existing isolated repair directory is required")
    thread = read_author(rpc, route)
    if thread["status"]["type"] == "notLoaded":
        rpc.request("thread/resume", {"threadId": route["thread_id"], "excludeTurns": True})
        thread = read_author(rpc, route)
    if thread["status"]["type"] != "idle":
        receipt.update(state="waiting_idle", observed_status=thread["status"])
        return
    prompt = prompt or (f"用户已授权PR修复闭环。先单独回复精确回执：\nPR_QUEUE_ACK {request['id']}\n"
              f"然后处理以下反馈。源码仅允许在指定修复工作树 {route['repair_cwd']} 写入；"
              "验证日志和回执可写入反馈指定的源码外目录。"
              "保留其他工作树及日用客户端。完成回执不表示修复已通过检查。\n\n"
              + request["body"])
    receipt.update(state="delivery_unknown", attempted_at=now())
    receipt.setdefault("attempts", []).append({"at": now(), "state": "sending"})
    save(output, receipt)
    try:
        response = rpc.request("turn/start", {
            "threadId": route["thread_id"], "clientUserMessageId": request["id"],
            "cwd": route["repair_cwd"],
            "input": [{"type": "text", "text": prompt, "text_elements": []}],
        })
    except Rejected:
        receipt["state"] = "rejected"
        raise
    receipt.update(state="sent", turn_id=response["turn"]["id"], sent_at=now())


def execute(action, route, request, output):
    uuid.UUID(request["id"])
    if not request["body"].strip():
        raise ValueError("Nonempty feedback is required")
    expected = {
        "request_id": request["id"], "thread_id": route["thread_id"],
        "body_sha256": hashlib.sha256(request["body"].encode()).hexdigest(),
        "route_sha256": hashlib.sha256(json.dumps(route, sort_keys=True).encode()).hexdigest(),
    }
    receipt = json.loads(output.read_text()) if output.exists() else {
        **expected, "state": "pending", "created_at": now(),
    }
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError("Feedback identity or route changed; use a new feedback ID")
    output.parent.mkdir(parents=True, exist_ok=True)
    save(output, receipt)
    try:
        with connect(route) as rpc:
            read_author(rpc, route, check_cwd=False)
            if action == "send":
                send(rpc, route, request, receipt, output)
            else:
                observe(rpc, route, receipt)
    except Exception as error:
        receipt.setdefault("errors", []).append({
            "at": now(), "type": type(error).__name__, "detail": str(error),
        })
        raise
    finally:
        save(output, receipt)
    return receipt


def run(action, route, request, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.with_suffix(".lock").open("a") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        return execute(action, route, request, output)


def main():
    parser = argparse.ArgumentParser(description="将明确授权的PR反馈投递至已核验的原会话")
    parser.add_argument("action", choices=("send", "poll"))
    parser.add_argument("--route", required=True, type=Path)
    parser.add_argument("--request", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    result = run(arguments.action, json.loads(arguments.route.read_text()),
                 json.loads(arguments.request.read_text()), arguments.output.resolve())
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
