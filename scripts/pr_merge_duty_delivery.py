import json
from pathlib import Path
import uuid

from scripts import pr_queue_transport as transport
from scripts import pr_task_native as native
from scripts.pr_task_state import ROOT, digest, load, now, save


def deliver(directory, route, key, message, skill=None):
    request_id = str(uuid.uuid5(uuid.NAMESPACE_URL, digest(key)))
    uuid.UUID(route["thread_id"])
    path = directory / f"delivery-{request_id}.json"
    receipt = json.loads(path.read_text()) if path.exists() else {
        "state": "pending", "request_id": request_id, "thread_id": route["thread_id"],
        "message_sha256": digest(message),
    }
    if (receipt["thread_id"], receipt["request_id"], receipt["message_sha256"]) != (
            route["thread_id"], request_id, digest(message)):
        raise ValueError("Existing receipt belongs to a different session or result")
    if receipt["state"] == "delivered":
        return receipt
    receipt["last_checked_at"] = now()
    try:
        with transport.connect_endpoint(route) as rpc:
            if receipt.get("attempted_at") and receipt["state"] not in {"waiting_idle", "rejected"}:
                transport.observe(rpc, route, receipt)
                if receipt.get("turn_id") and receipt.get("acknowledged_at"):
                    receipt["state"] = "delivered"
            elif native.is_archived(rpc, route):
                receipt["state"] = "archived"
            else:
                if receipt["state"] == "archived":
                    receipt["state"] = "pending"
                prompt = f"{message}\n处理后在单独一行回复 `PR_QUEUE_ACK {request_id}`。"
                arguments = {"prompt": prompt}
                if skill is not None:
                    arguments["skill"] = skill
                transport.send(rpc, route, {"id": request_id, "body": message}, receipt, path, **arguments)
        receipt.pop("error", None)
    except Exception as error:
        receipt["error"] = str(error)
    save(path, receipt)
    return receipt


def dispatch(config, directory, assignment, result, *, route=None):
    repository, number, head = config["repository"], assignment["pr"], assignment["head"]
    outcome = {}
    feedback = result.get("feedback")
    if (result["result"] in {"needs-author", "needs-evidence"} and isinstance(feedback, dict)
            and feedback.get("comment") and feedback.get("head", head) == head
            and feedback["kind"] in {"merge", "candidate", "review", "ci"}):
        task = load(Path(assignment["author_task"]).resolve(strict=True)) if assignment.get("author_task") else {}
        key = [repository, number, head, "failed", feedback["id"]]
        if task and task.get("mode", "automatic") == "automatic":
            prefix = "pr-task:feedback " if feedback["kind"] in {"review", "ci"} else "pr-queue:"
            marker = f"<!-- {prefix}{feedback['id']} -->"
            event = next((entry for entry in task["events"]
                          if entry["head"] == head and any(
                              item["kind"] == "feedback" and marker in item.get("body", "")
                              for item in entry["items"])), None)
            outcome["author"] = event.get("delivery", {"state": "waiting-for-author-tick"}) if event else {
                "state": "waiting-for-author-tick",
            }
        elif "socket" in (route or assignment.get("author_route") or {}):
            route = route or assignment["author_route"]
            if task and route["thread_id"] != task["route"]["thread_id"]:
                raise ValueError("Manual author's route does not match registered UUID")
            message = f"PR #{number} 本轮合并失败。反馈 {feedback['comment']}。"
            if config.get("mode") == "production-duty":
                message += (f"请原作者在既有独立工作树修复准确头 `{head}`，按反馈补回归，"
                            "使用原冻结检查器验证，并沿原分支提交。保留日用客户端。"
                            "请用项目现有 `pr-task` 登记原作者会话并交付新头，已有作者任务就用原任务，"
                            "保留准确头的交付报告，供值班员安排独立复审。")
            else:
                message += "仅记录这条通知。"
            outcome["author"] = deliver(directory, route, key, message)
        else:
            outcome["author"] = {"state": "unbound"}
        if config.get("duty_route"):
            outcome["duty"] = deliver(directory, config["duty_route"], [*key, "duty"],
                                      f"PR #{number} 的值班结果需要解释。"
                                      f"读取 `{directory / 'last-tick.json'}` 中编号 `{feedback['id']}` 的结果。",
                                      skill={"name": "pr-merge-duty", "path": str(
                                          ROOT / ".agents/skills/pr-merge-duty/SKILL.md")})
    candidate = result.get("candidate") or {}
    if result["result"] == "merged" and candidate.get("status") == "merged":
        task = load(Path(assignment["author_task"]).resolve(strict=True)) if assignment.get("author_task") else {}
        route = route or assignment.get("author_route") or task.get("route") or {}
        if "socket" not in route:
            outcome["author"] = {"state": "unbound"}
        else:
            if task and route["thread_id"] != task["route"]["thread_id"]:
                raise ValueError("Author route does not match registered UUID")
            route = {**route, "repair_cwd": route["expected_cwd"]}
            key = [repository, number, head, "merged", candidate["candidate"]]
            outcome["author"] = deliver(directory, route, key,
                                        f"PR #{number} 合并成功。源码 `{candidate['candidate']}`。仅记录这条通知。")
    return outcome
