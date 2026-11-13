import io
import json
import statistics
import struct
import time
import unittest

import remote_turn_proxy as proxy


def message(status="completed", body="private"):
    return {"id": 7, "result": {"data": [{
        "id": "turn", "status": status, "startedAt": 1.25, "completedAt": 2.5,
        "items": [
            {"type": "userMessage", "id": "first", "content": body},
            {"type": "commandExecution", "id": "tool", "aggregatedOutput": body},
            {"type": "userMessage", "id": "followup", "content": body},
            {"type": "agentMessage", "id": "final", "phase": "final_answer", "text": body},
            {"type": "agentMessage", "id": "later", "phase": "commentary", "text": body}
        ]
    }], "nextCursor": None}}


def projection(value):
    turn = value["result"]["data"][0]
    return {key: turn.get(key) for key in ("id", "status", "startedAt", "completedAt")} | {
        "users": sorted(item["id"] for item in turn["items"] if item["type"] == "userMessage"),
        "final": any(item["type"] == "agentMessage" and item.get("phase") == "final_answer"
                     for item in turn["items"])
    }


def wire(payload, opcode=1, final=True):
    output = io.BytesIO()
    proxy.frame(output, opcode, payload)
    result = output.getvalue()
    return result if final else bytes([result[0] & 0x7f]) + result[1:]


class RemoteSummaryTests(unittest.TestCase):
    def test_preserves_all_inputs_final_evidence_and_each_execution_status(self):
        for status in ("inProgress", "completed", "failed", "interrupted"):
            original = message(status)
            compact = json.loads(proxy.compact(json.dumps(original).encode()))
            self.assertEqual(projection(original), projection(compact))
            self.assertNotIn("private", json.dumps(compact))
            self.assertEqual(compact["result"]["nextCursor"], None)
        for unchanged in ({"method": "item/started", "params": {"item": "private"}},
                          {"id": 1, "error": {"code": -1, "message": "failure"}},
                          {"id": 2, "result": {"data": [{"id": "thread", "status": {}}]}}):
            payload = json.dumps(unchanged).encode()
            self.assertEqual(proxy.compact(payload), payload)

    def test_fragmentation_control_binary_and_extended_lengths(self):
        header = b"HTTP/1.1 101 Switching Protocols\r\n\r\n"
        for length in (125, 126, 65536):
            payload = json.dumps(message(body="x" * length)).encode()
            source = io.BytesIO(header + wire(payload[:17], final=False) + wire(b"ping", 9)
                                + wire(payload[17:], 0) + wire(b"\x00\xff", 2) + wire(b"", 8))
            output = io.BytesIO()
            proxy.relay(source, output)
            expected = header + wire(b"ping", 9) + wire(proxy.compact(payload))
            self.assertEqual(output.getvalue(), expected + wire(b"\x00\xff", 2) + wire(b"", 8))
        for invalid in (b"\xc1\x00", b"\x81\x80", b"\x80\x00",
                        b"\x81\x7f" + struct.pack("!Q", proxy.LIMIT + 1)):
            with self.assertRaises(ValueError):
                proxy.relay(io.BytesIO(header + invalid), io.BytesIO())

    def test_summary_transfer_and_processing_performance_budget(self):
        payload = json.dumps(message(body="x" * 750000)).encode()
        elapsed = []
        for _ in range(9):
            started = time.perf_counter()
            compact = proxy.compact(payload)
            elapsed.append(time.perf_counter() - started)
        self.assertLess(len(compact), 600)
        self.assertLess(len(compact) / len(payload), 0.001)
        self.assertLess(statistics.median(elapsed), 0.08)
        self.assertEqual(projection(json.loads(payload)), projection(json.loads(compact)))


if __name__ == "__main__":
    unittest.main()
