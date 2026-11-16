import copy
from pathlib import Path
import unittest
from unittest import mock

from acceptance.pr_task_fixtures import saved, task
from scripts import pr_task_native as native
from scripts.pr_task_state import digest


class PRTaskNativeTests(unittest.TestCase):
    def test_registration_preserves_original_cwd_and_limits_repairs_to_same_repository(self):
        state, _ = task(self)
        original = Path(state["directory"])
        author = original / "author"
        author.mkdir()
        socket = original / "rpc.sock"
        socket.touch()
        thread = {"id": state["route"]["thread_id"], "cwd": str(original), "status": {"type": "active"}}
        with mock.patch.object(native, "endpoint_pid", return_value=1), mock.patch.object(
                native, "process_identity", return_value={"pid": 1}), mock.patch.object(
                native.transport, "connect_endpoint") as connection, mock.patch.object(
                native, "latest", return_value=(thread, None, [])), mock.patch.object(
                native, "git", return_value="/same/common") as git:
            connection.return_value.__enter__.return_value.request.return_value = {"thread": thread}
            route, _ = native.register(socket, thread["id"], author)
            self.assertEqual(route["expected_cwd"], str(original))
            self.assertEqual(route["repair_cwd"], str(author))
            git.side_effect = ["/original/common", "/different/common"]
            with self.assertRaisesRegex(ValueError, "different repositories"):
                native.register(socket, thread["id"], author)

    def archive_case(self):
        state, _ = task(self)
        state["archive"] = True
        thread = {"id": state["route"]["thread_id"], "status": {"type": "idle"}}
        turn = {"id": "initial", "status": "completed"}
        connection = mock.patch.object(native.transport, "connect_endpoint")
        rpc = connection.start().return_value.__enter__.return_value
        self.addCleanup(connection.stop)
        latest = mock.patch.object(native, "latest", return_value=(thread, turn, []))
        latest.start()
        self.addCleanup(latest.stop)
        return state, thread, turn, rpc

    def test_unrelated_input_blocks_delivery_and_archive(self):
        state, _ = task(self)
        with self.assertRaisesRegex(ValueError, "unrelated new input"):
            native.owned(state, {"id": "new"}, [{"type": "userMessage", "clientId": "foreign"}])
        state["events"] = [{"id": "owned"}]
        native.owned(state, {"id": "new"}, [{"type": "userMessage", "clientId": "owned"}])

    def test_busy_author_or_child_is_never_archived(self):
        for author, descendants in (("active", True), ("idle", False)):
            state, thread, _, rpc = self.archive_case()
            thread["status"]["type"] = author
            with mock.patch.object(native, "is_archived", return_value=False), mock.patch.object(
                    native, "descendants_idle", return_value=descendants):
                self.assertFalse(native.archive(state))
            rpc.request.assert_not_called()
            self.assertNotIn("archive_receipt", saved(state))

    def test_archive_response_alone_does_not_confirm_completion(self):
        state, _, _, rpc = self.archive_case()
        with mock.patch.object(native, "is_archived", return_value=False), mock.patch.object(
                native, "descendants_idle", return_value=True):
            with self.assertRaisesRegex(RuntimeError, "readback"):
                native.archive(state)
        rpc.request.assert_called_once_with("thread/archive", {"threadId": state["route"]["thread_id"]})
        self.assertEqual(saved(state)["archive_receipt"]["status"], "unknown")
        with mock.patch.object(native, "is_archived", return_value=True):
            self.assertTrue(native.archive(state))
        self.assertEqual(rpc.request.call_count, 1)
        self.assertEqual(saved(state)["archive_receipt"]["status"], "confirmed")

    def test_new_input_between_archive_checks_blocks_mutation(self):
        state, thread, turn, rpc = self.archive_case()
        later = (thread, {"id": "foreign", "status": "completed"}, [{"type": "userMessage"}])
        with mock.patch.object(native, "is_archived", return_value=False), mock.patch.object(
                native, "descendants_idle", return_value=True), mock.patch.object(
                native, "latest", side_effect=[(thread, turn, []), later]):
            with self.assertRaisesRegex(ValueError, "unrelated"):
                native.archive(state)
        rpc.request.assert_not_called()

    def test_archive_lookup_includes_exec_and_fails_repeated_cursor(self):
        state, _ = task(self)
        rpc = mock.Mock()
        rpc.request.return_value = {"data": [], "nextCursor": "same"}
        with self.assertRaisesRegex(ValueError, "repeated a cursor"):
            native.is_archived(rpc, state["route"])
        self.assertIn("exec", rpc.request.call_args.args[1]["sourceKinds"])

    def test_ephemeral_loaded_descendant_blocks_archive_even_when_absent_from_catalog(self):
        state, _ = task(self)
        root = state["route"]["thread_id"]
        def response(method, parameters):
            if method == "thread/list":
                self.assertEqual(parameters["ancestorThreadId"], root)
                self.assertNotIn("sourceKinds", parameters)
                return {"data": []}
            if method == "thread/loaded/list":
                return {"data": [root, "child", "unrelated"]}
            return {"thread": {"id": parameters["threadId"],
                               "parentThreadId": root if parameters["threadId"] == "child" else None,
                               "status": {"type": "active"}}}
        rpc = mock.Mock(request=mock.Mock(side_effect=response))
        self.assertFalse(native.descendants_idle(rpc, state["route"]))

    def test_delivery_unknown_is_persisted_and_never_blindly_resent(self):
        state, _ = task(self)
        event = {"id": "event", "prompt": "repair"}
        state["events"].append(event)
        def send(rpc, route, request, receipt, output, **kwargs):
            receipt.update(state="delivery_unknown", attempted_at="now")
            raise TimeoutError("reply lost")
        with mock.patch.object(native.transport, "connect_endpoint"), mock.patch.object(
                native, "latest", return_value=({}, {"id": "initial"}, [])), mock.patch.object(
                native.transport, "send", side_effect=send) as sending, mock.patch.object(
                native.transport, "observe") as observe:
            with self.assertRaises(TimeoutError):
                native.delivery(state, event)
            self.assertEqual(saved(state)["events"][0]["delivery"]["state"], "delivery_unknown")
            native.delivery(state, event)
        self.assertEqual(sending.call_count, 1)
        observe.assert_called_once()

    def test_route_change_or_prompt_change_cannot_reuse_delivery_receipt(self):
        state, _ = task(self)
        event = {"id": "event", "prompt": "repair"}
        state["events"].append(event)
        with mock.patch.object(native.transport, "connect_endpoint"), mock.patch.object(
                native, "latest", return_value=({}, {"id": "initial"}, [])), mock.patch.object(
                native.transport, "send"):
            native.delivery(state, event)
        original = copy.deepcopy(state["route"])
        for change in ("route", "prompt"):
            state["route"] = {**original, "socket": "/different"} if change == "route" else original
            event["prompt"] = "different" if change == "prompt" else "repair"
            with self.assertRaisesRegex(ValueError, "changed"):
                native.delivery(state, event)
