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
            self.assertEqual(transport.run("poll", route, request, output)["state"], "completed")

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


if __name__ == "__main__":
    unittest.main()
