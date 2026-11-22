import assert from "node:assert/strict";
import { test } from "node:test";
import {
    Archive,
    ArchivePreconditionError,
    type ArchiveTarget,
    type BoardLedger,
    emptyLedger,
} from "./archive.js";

function harness() {
    let ledger: BoardLedger = emptyLedger();
    let scope = "account-a";
    let writes = 0;
    const checked: string[] = [];
    const archived = new Set<string>();
    const create = () =>
        new Archive({
            scope: () => scope,
            ledger: () => ledger,
            nativeVersion: (version) => version ?? "",
            async write(next) {
                ledger = JSON.parse(JSON.stringify(next)) as BoardLedger;
            },
            async verify() {},
            async isArchived(id) {
                checked.push(id);
                return archived.has(id);
            },
            async archive({ id }) {
                if (id === "rejected") throw new ArchivePreconditionError("位置已变更");
                writes++;
                throw new Error("归档回执丢失");
            },
        });
    return {
        create,
        archived,
        checked,
        writes: () => writes,
        changeScope: () => {
            scope = "account-b";
        },
    };
}

const item = (id: string): ArchiveTarget => ({
    id,
    title: id,
    version: 1,
    inputVersion: 2,
    sectionVersion: "expected",
});

test("reopening an incomplete archive batch checks submitted items and never resends them", async () => {
    const fixture = harness();
    const original = fixture.create();
    const batchId = crypto.randomUUID();
    const first = item("sent");
    const untouched = item("not sent");
    await original.start(batchId, [first, untouched]);
    assert.equal((await original.submit(batchId, first)).status, "pending");
    assert.equal(fixture.writes(), 1);
    const reopened = fixture.create();
    fixture.archived.add(first.id);
    assert.deepEqual(
        (await reopened.check(batchId)).items.map(({ status }) => status),
        ["verified", "awaiting"],
    );
    assert.deepEqual(fixture.checked, [first.id, first.id]);
    await reopened.finish(batchId);
    assert.equal((await reopened.submit(batchId, first)).status, "verified");
    await assert.rejects(reopened.submit(batchId, untouched), /已有归档记录/);
    assert.equal(fixture.writes(), 1);
});

test("a second archive precondition failure remains distinct from a possible application", async () => {
    const fixture = harness();
    const original = fixture.create();
    const candidate = item("rejected");
    const first = await original.single(candidate);
    assert.equal(first.status, "rejected");
    assert.equal(fixture.writes(), 0);
    assert.deepEqual(fixture.checked, []);
    assert.deepEqual(await original.single(candidate), first);
    assert.equal(original.batches().length, 1);
});

test("an archive ledger update queued before an account switch cannot commit after it", async () => {
    const fixture = harness();
    const archive = fixture.create();
    const task = archive.start(crypto.randomUUID(), [item("stale")]);
    fixture.changeScope();
    await assert.rejects(task, /账号已变更/);
    assert.deepEqual(archive.batches(), []);
});
