import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import uuid

from scripts import pr_queue_transport as transport


class QueueTransportTests(unittest.TestCase):
    def setup_case(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        request = {"id": str(uuid.uuid4()), "body": "Repair the pinned conflict"}
        route = {"thread_id": "original", "expected_cwd": str(root),
                 "repair_cwd": str(root), "coordinator_thread": "coordinator"}
        return root / "receipt.json", route, request

    def rpc(self, route, status="idle"):
        def request(method, parameters):
            if method == "thread/read":
                return {"thread": {"id": route["thread_id"],
                                   "cwd": route["expected_cwd"], "status": {"type": status}}}
            if method == "turn/start":
                return {"turn": {"id": "delivered"}}
            if method == "thread/items/list":
                return {"data": []}
            if method == "thread/turns/list":
                return {"data": [{"id": "delivered", "status": "inProgress"}]}
            raise AssertionError(method)
        return mock.Mock(request=mock.Mock(side_effect=request))

    def rpc_without_items(self, route, thread, error=None):
        original = self.rpc(route).request.side_effect
        error = error or {"code": -32601, "message": "thread/items/list is not supported yet"}
        def request(method, parameters):
            if method == "thread/read" and parameters.get("includeTurns"):
                return {"thread": thread}
            if method == "thread/items/list":
                connection = mock.Mock()
                if isinstance(error, Exception):
                    connection.recv.side_effect = error
                else:
                    connection.recv.return_value = json.dumps({"id": 1, "error": error})
                return transport.Rpc(connection).request(method, parameters)
            return original(method, parameters)
        return mock.Mock(request=mock.Mock(side_effect=request))

    def test_unsupported_items_reads_only_the_exact_original_turn_assistant_ack(self):
        output, route, request = self.setup_case()
        ack = {"type": "agentMessage", "id": "ack",
               "text": f"PR_QUEUE_ACK {request['id']}", "completedAtMs": None}
        target = {"id": "delivered", "status": "inProgress",
                  "items": [{**ack, "type": "userMessage"}, {**ack, "text": "PR_QUEUE_ACK wrong"}]}
        thread = {"id": route["thread_id"], "turns": [
            {"id": "unrelated", "status": "completed", "items": [ack]}, target]}
        rpc = self.rpc_without_items(route, thread)
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            transport.run("send", route, request, output)
            self.assertEqual(transport.run("poll", route, request, output)["state"], "sent")
            target["items"].append(ack)
            accepted = transport.run("poll", route, request, output)
            self.assertEqual(accepted["state"], "acknowledged")
            self.assertEqual(accepted["ack_item_id"], "ack")
            self.assertEqual(accepted["ack_time_basis"], "observed_upper_bound")
            self.assertEqual(accepted["observation_method"], "thread/read")
            target["status"] = "completed"
            completed = transport.run("poll", route, request, output)
            self.assertEqual(completed["state"], "author_turn_completed")
            repeated = transport.run("poll", route, request, output)
            self.assertEqual(repeated["acknowledged_at"], accepted["acknowledged_at"])
            self.assertEqual(repeated["completed_at"], completed["completed_at"])
        self.assertEqual([call.args[0] for call in rpc.request.call_args_list].count("turn/start"), 1)
        self.assertNotIn("thread/turns/list", [call.args[0] for call in rpc.request.call_args_list])

    def test_unsupported_items_keeps_unknown_delivery_until_unique_user_receipt(self):
        output, route, request = self.setup_case()
        target = {"id": "delivered", "status": "inProgress", "items": [
            {"type": "agentMessage", "id": "ack", "text": f"PR_QUEUE_ACK {request['id']}"}]}
        thread = {"id": route["thread_id"], "turns": [target]}
        rpc = self.rpc_without_items(route, thread)
        original = rpc.request.side_effect
        def lost_start(method, parameters):
            if method == "turn/start":
                raise TimeoutError("start response lost")
            return original(method, parameters)
        rpc.request.side_effect = lost_start
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            with self.assertRaises(TimeoutError):
                transport.run("send", route, request, output)
            unknown = transport.run("send", route, request, output)
            self.assertEqual(unknown["state"], "delivery_unknown")
            self.assertNotIn("turn_id", unknown)
            self.assertNotIn("acknowledged_at", unknown)
            user = {"type": "userMessage", "clientId": request["id"]}
            target["items"].append(user)
            thread["turns"].append({"id": "other", "status": "completed", "items": [user]})
            with self.assertRaisesRegex(ValueError, "multiple turns"):
                transport.run("poll", route, request, output)
            self.assertEqual(json.loads(output.read_text())["state"], "delivery_unknown")
            thread["turns"].pop()
            accepted = transport.run("poll", route, request, output)
            self.assertEqual(accepted["state"], "acknowledged")
            self.assertEqual(accepted["turn_id"], "delivered")
        self.assertEqual([call.args[0] for call in rpc.request.call_args_list].count("turn/start"), 1)

    def test_fallback_does_not_hide_rpc_errors_or_accept_wrong_thread_or_turn(self):
        failures = [
            ("wrong_thread", None),
            ("missing_turn", None),
            ("duplicate_turn", None),
            ("permission", {"code": -32601, "message": "permission denied"}),
            ("other_code", {"code": -32000, "message": "thread/items/list is not supported yet"}),
            ("timeout", TimeoutError("items timeout")),
        ]
        for scenario, error in failures:
            with self.subTest(scenario=scenario):
                output, route, request = self.setup_case()
                target = {"id": "delivered", "status": "completed", "items": [
                    {"type": "agentMessage", "id": "ack", "text": f"PR_QUEUE_ACK {request['id']}"}]}
                thread = {"id": route["thread_id"], "turns": [target]}
                if scenario == "wrong_thread":
                    thread["id"] = "another"
                elif scenario == "missing_turn":
                    target["id"] = "another"
                elif scenario == "duplicate_turn":
                    thread["turns"].append(target)
                rpc = self.rpc_without_items(route, thread, error)
                with mock.patch.object(transport, "connect") as connection:
                    connection.return_value.__enter__.return_value = rpc
                    transport.run("send", route, request, output)
                    expected = TimeoutError if scenario == "timeout" else (
                        transport.Rejected if error else ValueError)
                    with self.assertRaises(expected):
                        transport.run("poll", route, request, output)
                receipt = json.loads(output.read_text())
                self.assertEqual(receipt["state"], "sent")
                self.assertNotIn("acknowledged_at", receipt)
                if error:
                    self.assertFalse(any(call.args[1].get("includeTurns")
                                         for call in rpc.request.call_args_list))

    def test_busy_author_is_not_interrupted_or_reported_as_delivered(self):
        output, route, request = self.setup_case()
        rpc = self.rpc(route, "active")
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            receipt = transport.run("send", route, request, output)
        self.assertEqual(receipt["state"], "waiting_idle")
        self.assertNotIn("turn/start", [call.args[0] for call in rpc.request.call_args_list])

    def test_lost_start_receipt_is_not_retried_and_can_be_reconciled(self):
        output, route, request = self.setup_case()
        rpc = self.rpc(route)
        initial = rpc.request.side_effect
        def disconnect(method, parameters):
            if method == "turn/start":
                self.assertEqual(json.loads(output.read_text())["state"], "delivery_unknown")
                raise TimeoutError("connection lost after acceptance")
            return initial(method, parameters)
        rpc.request.side_effect = disconnect
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            with self.assertRaises(TimeoutError):
                transport.run("send", route, request, output)
            rpc.request.reset_mock()
            rpc.request.side_effect = initial
            receipt = transport.run("send", route, request, output)
            self.assertEqual(receipt["state"], "delivery_unknown")
            self.assertNotIn("turn/start", [call.args[0] for call in rpc.request.call_args_list])
            def recovered(method, parameters):
                if method == "thread/items/list" and not parameters.get("turnId"):
                    if not parameters.get("cursor"):
                        return {"data": [], "nextCursor": "older"}
                    return {"data": [{"turnId": "delivered", "item": {
                        "type": "userMessage", "clientId": request["id"]}}]}
                return initial(method, parameters)
            rpc.request.side_effect = recovered
            self.assertEqual(transport.run("poll", route, request, output)["state"], "sent")

    def test_only_original_turn_assistant_acknowledges_and_completed_is_separate(self):
        output, route, request = self.setup_case()
        rpc = self.rpc(route)
        initial = rpc.request.side_effect
        item = {"type": "userMessage", "text": f"PR_QUEUE_ACK {request['id']}", "id": "ack"}
        status = "inProgress"
        def replies(method, parameters):
            if method == "thread/items/list":
                return {"data": [{"turnId": "delivered", "item": item}]}
            if method == "thread/turns/list":
                return {"data": [{"id": "delivered", "status": status}]}
            return initial(method, parameters)
        rpc.request.side_effect = replies
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            self.assertEqual(transport.run("send", route, request, output)["state"], "sent")
            self.assertEqual(transport.run("poll", route, request, output)["state"], "sent")
            item["type"] = "agentMessage"
            self.assertEqual(transport.run("poll", route, request, output)["state"], "acknowledged")
            status = "completed"
            completed = transport.run("poll", route, request, output)
            self.assertEqual(completed["state"], "author_turn_completed")
            self.assertEqual(transport.run("poll", route, request, output)["completed_at"],
                             completed["completed_at"])

    def test_wrong_author_identity_or_directory_prevents_turn_start(self):
        output, route, request = self.setup_case()
        for field, value in (("id", "unrelated"), ("cwd", "/another/project")):
            rpc = self.rpc(route)
            thread = rpc.request("thread/read", {})["thread"]
            thread[field] = value
            rpc.request.side_effect = None
            rpc.request.return_value = {"thread": thread}
            with mock.patch.object(transport, "connect") as connection:
                connection.return_value.__enter__.return_value = rpc
                with self.assertRaisesRegex(ValueError, "does not match"):
                    transport.run("send", route, request, output)
            self.assertNotIn("turn/start", [call.args[0] for call in rpc.request.call_args_list])

    def test_changed_feedback_cannot_reuse_receipt_or_write_to_another_thread(self):
        output, route, request = self.setup_case()
        rpc = self.rpc(route)
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            transport.run("send", route, request, output)
            for changed_route, changed_request in (
                ({**route, "thread_id": "another"}, request),
                (route, {**request, "body": "different repair"}),
            ):
                with self.assertRaisesRegex(ValueError, "identity or route changed"):
                    transport.run("send", changed_route, changed_request, output)

    def test_receipt_remains_readable_after_author_moves_to_another_directory(self):
        output, route, request = self.setup_case()
        repair = output.parent / "repair"
        repair.mkdir()
        route["repair_cwd"] = str(repair)
        rpc = self.rpc(route)
        original = rpc.request.side_effect
        with mock.patch.object(transport, "connect") as connection:
            connection.return_value.__enter__.return_value = rpc
            transport.run("send", route, request, output)
            repair.rmdir()
            def moved(method, parameters):
                if method == "thread/read":
                    thread = original(method, parameters)["thread"]
                    return {"thread": {**thread, "cwd": "/another/project"}}
                if method == "thread/items/list":
                    return {"data": [{"turnId": "delivered", "item": {
                        "type": "agentMessage", "id": "ack",
                        "text": f"PR_QUEUE_ACK {request['id']}"}}]}
                if method == "thread/turns/list":
                    return {"data": [{"id": "delivered", "status": "completed"}]}
                return original(method, parameters)
            rpc.request.side_effect = moved
            rpc.request.reset_mock()
            self.assertEqual(transport.run("poll", route, request, output)["state"],
                             "author_turn_completed")
            self.assertEqual(transport.run("send", route, request, output)["state"],
                             "author_turn_completed")
            self.assertNotIn("turn/start", [call.args[0] for call in rpc.request.call_args_list])


if __name__ == "__main__":
    unittest.main()
