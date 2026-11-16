from pathlib import Path
import time
import uuid

from scripts import pr_queue_transport as transport
from scripts.pr_task_state import digest, now, persist, save
from scripts.shared_desktop import endpoint_pid, process_identity


def latest(rpc, route):
    if not hasattr(rpc, "deadline"):
        rpc.deadline = time.monotonic() + 60
    thread = transport.read_author(rpc, route)
    page = rpc.request("thread/turns/list", {
        "threadId": route["thread_id"], "limit": 1, "sortDirection": "desc", "itemsView": "full",
    })
    turns = page["data"]
    if not isinstance(turns, list) or len(turns) > 1:
        raise ValueError("Native latest-turn response is invalid")
    turn = turns[0] if turns else None
    if turn is not None and (not turn.get("id") or not isinstance(turn.get("items"), list)):
        raise ValueError("Native latest-turn inputs are unavailable")
    users = [item for item in turn["items"] if item["type"] == "userMessage"] if turn else []
    return thread, turn, users


def register(socket_path, thread_id, project):
    uuid.UUID(thread_id)
    socket_path = Path(socket_path).resolve(strict=True)
    route = {"socket": str(socket_path), "backend": process_identity(endpoint_pid(socket_path)),
             "thread_id": thread_id, "expected_cwd": str(project), "repair_cwd": str(project)}
    with transport.connect_endpoint(route) as rpc:
        thread, turn, users = latest(rpc, route)
    if thread["status"]["type"] not in {"active", "idle"}:
        raise ValueError("Register from the original loaded author session")
    return route, {"turn": turn["id"] if turn else None, "users": digest(users)}


def owned(state, turn, users):
    anchor = state["session_anchor"]
    if (turn["id"] if turn else None) == anchor["turn"] and digest(users) == anchor["users"]:
        return
    requests = {entry["id"] for entry in state["events"]}
    if not users or any(item.get("clientId") not in requests for item in users):
        raise ValueError("Original session has unrelated new input; explicitly rebind before continuing")


def request_for(state, event):
    return {"id": event["id"], "body": event["prompt"]}


def receipt_path(state, event):
    return Path(state["directory"]) / event.get("delivery_file", f"delivery-{event['id']}.json")


def delivery(state, event):
    request = request_for(state, event)
    output = receipt_path(state, event)
    expected = {"request_id": event["id"], "thread_id": state["route"]["thread_id"],
                "body_sha256": digest(request["body"]), "route_sha256": digest(state["route"])}
    import json
    receipt = json.loads(output.read_text()) if output.exists() else {
        **expected, "state": "pending", "created_at": now(),
    }
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError("Delivery request or registered backend changed")
    save(output, receipt)
    try:
        with transport.connect_endpoint(state["route"]) as rpc:
            rpc.deadline = time.monotonic() + 90
            if receipt["state"] in {"pending", "waiting_idle", "rejected"}:
                _, turn, users = latest(rpc, state["route"])
                owned(state, turn, users)
                transport.send(rpc, state["route"], request, receipt, output, prompt=request["body"])
            else:
                transport.observe(rpc, state["route"], receipt)
    finally:
        save(output, receipt)
        event["delivery"] = receipt
        persist(state)
    return receipt


def is_archived(rpc, route):
    cursor, seen = None, set()
    for _ in range(100):
        page = rpc.request("thread/list", {
            "archived": True, "useStateDbOnly": True, "limit": 100, "cursor": cursor,
            "sortKey": "updated_at", "sortDirection": "desc", "modelProviders": [],
            "sourceKinds": ["cli", "vscode", "appServer", "exec", "subAgent", "subAgentReview",
                            "subAgentCompact", "subAgentThreadSpawn", "subAgentOther", "unknown"],
        })
        if not isinstance(page["data"], list):
            raise ValueError("Native archive listing is incomplete")
        if any(entry["id"] == route["thread_id"] for entry in page["data"]):
            return True
        cursor = page.get("nextCursor")
        if not cursor:
            return False
        if cursor in seen:
            raise ValueError("Native archive listing repeated a cursor")
        seen.add(cursor)
    raise ValueError("Native archive listing exceeded its bound")


def collection(rpc, method, parameters):
    cursor, seen, entries = None, set(), []
    for _ in range(100):
        page = rpc.request(method, {**parameters, "limit": 100, "cursor": cursor})
        if not isinstance(page["data"], list):
            raise ValueError("Native descendant listing is incomplete")
        entries.extend(page["data"])
        cursor = page.get("nextCursor")
        if not cursor:
            return entries
        if cursor in seen:
            raise ValueError("Native descendant listing repeated a cursor")
        seen.add(cursor)
    raise ValueError("Native descendant listing exceeded its bound")


def descendants_idle(rpc, route):
    root = route["thread_id"]
    descendants = set()
    for archived in (False, True):
        descendants.update(entry["id"] for entry in collection(
            rpc, "thread/list", {"ancestorThreadId": root, "archived": archived}))
    loaded = collection(rpc, "thread/loaded/list", {})
    nodes = {}

    def read(identifier):
        if identifier not in nodes:
            nodes[identifier] = rpc.request("thread/read", {
                "threadId": identifier, "includeTurns": False,
            })["thread"]
        thread = nodes[identifier]
        if thread["id"] != identifier or "parentThreadId" not in thread:
            raise ValueError("Backend cannot prove the native archive descendant graph")
        return thread

    def belongs(identifier):
        visited = set()
        while identifier and identifier != root:
            if identifier in visited or len(visited) >= 100:
                raise ValueError("Native parent graph is cyclic or exceeds its bound")
            visited.add(identifier)
            identifier = read(identifier)["parentThreadId"]
        return identifier == root

    for identifier in loaded:
        if identifier != root and belongs(identifier):
            descendants.add(identifier)
    for identifier in descendants:
        if identifier == root or not belongs(identifier):
            raise ValueError("Backend did not apply the ancestor scope")
        status = read(identifier)["status"]["type"]
        if status == "active":
            return False
        if status not in {"idle", "notLoaded"}:
            raise ValueError("Descendant status is not known to be idle")
    return True


def archive(state):
    if not state["archive"]:
        state["archive_receipt"] = {"status": "not_requested"}
        persist(state)
        return True
    with transport.connect_endpoint(state["route"]) as rpc:
        rpc.deadline = time.monotonic() + 90
        if is_archived(rpc, state["route"]):
            state["archive_receipt"] = {"status": "confirmed", "at": now()}
            persist(state)
            return True
        thread, turn, users = latest(rpc, state["route"])
        owned(state, turn, users)
        if (thread["status"]["type"] not in {"idle", "notLoaded"}
                or turn and turn["status"] not in {"completed", "failed", "interrupted"}):
            return False
        if not descendants_idle(rpc, state["route"]):
            return False
        thread, turn, users = latest(rpc, state["route"])
        owned(state, turn, users)
        if (thread["status"]["type"] not in {"idle", "notLoaded"}
                or turn and turn["status"] not in {"completed", "failed", "interrupted"}):
            return False
        state["archive_receipt"] = {"status": "unknown", "at": now()}
        persist(state)
        rpc.request("thread/archive", {"threadId": state["route"]["thread_id"]})
        if not is_archived(rpc, state["route"]):
            raise RuntimeError("Archive request returned but native archive readback did not confirm it")
        state["archive_receipt"] = {"status": "confirmed", "at": now()}
        persist(state)
        return True
