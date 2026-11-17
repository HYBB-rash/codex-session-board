import unittest
from copy import deepcopy
from uuid import UUID

from scripts.four_column_migration import STATES, advance, start


def identifier(number):
    return str(UUID(int=number))


def bindings(base):
    return {state: identifier(base + index) for index, state in enumerate(sorted(STATES))}


class FourColumnMigrationTest(unittest.TestCase):
    def setUp(self):
        self.old = identifier(1)
        self.keys = [{"hostId": "bb", "threadId": identifier(number)} for number in (2, 3)]
        self.plan = {
            "accountId": "test-account", "oldSectionId": self.old, "bindings": bindings(100),
            "candidateSha256": "a" * 64, "backupSha256": "b" * 64,
            "hosts": {
                "bb": {"source": "c" * 64, "backendId": identifier(11), "bindings": bindings(200)},
                "local": {"source": "d" * 64, "backendId": identifier(12), "bindings": bindings(300)},
            },
            "moves": [{**self.keys[1], "state": None}, {**self.keys[0], "state": "blocked"}],
        }
        self.snapshot = {
            "accountId": self.plan["accountId"], "oldSectionId": self.old, "oldPresent": True,
            "bindings": deepcopy(self.plan["bindings"]), "oldMembers": deepcopy(self.keys),
            "cards": [{**key, "sectionId": self.old, "archived": False, "pinned": False,
                       "running": False, "version": 1, "inputVersion": 1, "turnRevision": "turn",
                       "positionRevision": "original", "metadataSha256": "f" * 64}
                      for key in self.keys],
            "observedAt": 1,
            "hosts": {host_id: {**deepcopy(host), "complete": True, "oldPresent": True,
                                "oldMembers": []} for host_id, host in self.plan["hosts"].items()},
            "fourColumnCandidate": None, "oldTargetRejected": False,
        }
        self.snapshot["hosts"]["bb"]["oldMembers"] = [key["threadId"] for key in self.keys]
        self.ledger = start(self.plan, deepcopy(self.snapshot))

    def next_action(self, **options):
        self.snapshot["observedAt"] += 1
        return advance(self.ledger, deepcopy(self.snapshot), **options)

    def observe_move(self, index, target):
        row = self.snapshot["cards"][index]
        if target == self.old:
            self.snapshot["oldMembers"].insert(index, self.keys[index])
        else:
            self.snapshot["oldMembers"].remove(self.keys[index])
        self.snapshot["hosts"]["bb"]["oldMembers"] = [
            key["threadId"] for key in self.snapshot["oldMembers"]]
        row.update(sectionId=target, version=row["version"] + 1,
                   positionRevision=str(row["version"] + 1))

    def migrate(self):
        for index in range(len(self.keys)):
            action = self.next_action()
            self.observe_move(index, action["arguments"]["sectionId"])
            self.assertEqual(self.next_action()["action"], "confirmed")

    def test_lost_move_receipt_and_native_default(self):
        first = self.next_action()
        self.assertEqual(first["arguments"]["sectionId"], self.plan["bindings"]["blocked"])
        self.assertEqual(first["arguments"]["threadId"], self.keys[0]["threadId"])
        with self.assertRaisesRegex(ValueError, "回执未确认"):
            self.next_action()
        self.assertEqual(self.next_action(retry=True), first)
        self.observe_move(0, first["arguments"]["sectionId"])
        self.assertEqual(self.next_action(retry=True)["action"], "confirmed")
        second = self.next_action()
        self.assertIsNone(second["arguments"]["sectionId"])
        self.assertEqual(second["arguments"]["hostId"], "bb")

    def test_unplanned_move_and_new_input_require_reconciliation(self):
        self.observe_move(0, self.plan["bindings"]["blocked"])
        with self.assertRaisesRegex(ValueError, "没有已记录的操作意图"):
            self.next_action()
        self.observe_move(0, self.old)
        self.snapshot["cards"][0] = deepcopy(self.ledger["moves"][0]["before"])
        action = self.next_action()
        self.observe_move(0, action["arguments"]["sectionId"])
        self.snapshot["cards"][0]["inputVersion"] += 1
        with self.assertRaisesRegex(ValueError, "位置或版本已改变"):
            self.next_action()

    def test_delete_requires_candidate_and_all_host_readbacks(self):
        self.migrate()
        with self.assertRaisesRegex(ValueError, "缺少已切换"):
            self.next_action(delete=True)
        self.snapshot.update(fourColumnCandidate=self.plan["candidateSha256"], oldTargetRejected=True)
        self.snapshot["hosts"]["local"]["complete"] = False
        with self.assertRaisesRegex(ValueError, "读回不完整"):
            self.next_action(delete=True)
        self.snapshot["hosts"]["local"]["complete"] = True
        self.assertEqual(self.next_action()["action"], "ready_to_delete")
        action = self.next_action(delete=True)
        self.assertEqual(action, {"action": "delete", "arguments": {"sectionId": self.old}})
        self.assertEqual(self.next_action(retry=True), action)
        self.snapshot["oldPresent"] = False
        self.snapshot["hosts"]["local"]["oldPresent"] = False
        with self.assertRaisesRegex(ValueError, "仍有旧后台分区"):
            self.next_action()
        self.snapshot["hosts"]["bb"]["oldPresent"] = False
        self.assertEqual(self.next_action()["action"], "deleted")
        with self.assertRaisesRegex(ValueError, "重新绑定新 ID"):
            self.next_action(rollback=True)

    def test_rollback_restores_order_and_preserves_later_edits(self):
        self.migrate()
        action = self.next_action(rollback=True)
        self.assertEqual(action["arguments"]["threadId"], self.keys[1]["threadId"])
        self.observe_move(1, self.old)
        self.assertEqual(self.next_action()["action"], "confirmed")
        self.snapshot["cards"][0]["metadataSha256"] = "e" * 64
        with self.assertRaisesRegex(ValueError, "位置或版本已改变"):
            self.next_action()
        self.snapshot["cards"][0]["metadataSha256"] = "f" * 64
        action = self.next_action()
        self.assertEqual(action["arguments"]["beforeThreadId"], self.keys[1]["threadId"])
        self.observe_move(0, self.old)
        self.assertEqual(self.next_action()["action"], "confirmed")
        self.assertEqual(self.next_action()["action"], "rolled_back")
        self.assertEqual(self.snapshot["oldMembers"], self.keys)

    def test_account_membership_and_freshness_are_required(self):
        with self.assertRaisesRegex(ValueError, "独立读回"):
            advance(self.ledger, self.snapshot)
        self.snapshot["hosts"]["bb"]["oldMembers"].pop()
        with self.assertRaisesRegex(ValueError, "侧栏与后台成员不一致"):
            self.next_action()
        self.snapshot["accountId"] = "another-account"
        with self.assertRaisesRegex(ValueError, "账号改变"):
            self.next_action()
