import assert from "node:assert/strict";
import { test } from "node:test";
import {
    ArchivePreconditionError,
    ArchiveReadUnavailable,
    type ArchiveTarget,
    type BoardLedger,
    createArchive,
    emptyLedger,
} from "./archive.js";

function harness(
    verify: () => Promise<void> = async () => {},
    respond: () => Promise<void> = async () => {
        throw new Error("归档回执丢失");
    },
    check: (id: string) => void = () => {},
) {
    let ledger: BoardLedger = emptyLedger();
    let scope = "account-a";
    let writes = 0;
    const checked: string[] = [];
    const archived = new Set<string>();
    const create = () =>
        createArchive({
            scope: () => scope,
            ledger: () => ledger,
            nativeVersion: (version) => version ?? "",
            async write(next) {
                ledger = JSON.parse(JSON.stringify(next)) as BoardLedger;
            },
            verify,
            async isArchived(id) {
                checked.push(id);
                check(id);
                return archived.has(id);
            },
            async archive({ id }) {
                if (id === "rejected") throw new ArchivePreconditionError("位置已变更");
                writes++;
                await respond();
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

test("reopening an incomplete archive batch checks submitted items and never resends them", async (testContext) => {
    let time = 1000;
    testContext.mock.method(Date, "now", () => time);
    const fixture = harness();
    const original = fixture.create();
    const batchId = crypto.randomUUID();
    const first = item("sent");
    const untouched = item("not sent");
    await original.start(batchId, [first, untouched]);
    const pending = await original.submit(batchId, first);
    assert.equal(pending.status, "pending");
    assert.equal(pending.checkedAt, 1);
    assert.equal(fixture.writes(), 1);
    const reopened = fixture.create();
    fixture.archived.add(first.id);
    time = 2500;
    assert.deepEqual(
        (await reopened.check(batchId)).items.map(({ status, checkedAt }) => [status, checkedAt]),
        [
            ["verified", 2.5],
            ["awaiting", undefined],
        ],
    );
    assert.deepEqual(fixture.checked, [first.id, first.id]);
    await reopened.finish(batchId);
    assert.equal((await reopened.submit(batchId, first)).status, "verified");
    await assert.rejects(reopened.submit(batchId, untouched), /已有归档记录/);
    assert.equal(fixture.writes(), 1);
});

test("a disconnected archive host leaves its receipt pending and does not block a healthy host", async (testContext) => {
    let time = 1000;
    testContext.mock.method(Date, "now", () => time);
    const host = "remote-ssh-discovered:archive-fixture";
    const local = item("00000000-0000-4000-8000-000000000031");
    const remote = item(JSON.stringify([host, local.id]));
    let connected = true;
    const fixture = harness(undefined, undefined, (id) => {
        if (!connected && id === remote.id)
            throw new ArchiveReadUnavailable(`Codex 连接已断开 (${host})`);
    });
    const batchId = crypto.randomUUID();
    const initial = fixture.create();
    await initial.start(batchId, [remote, local]);
    const pending = await initial.submit(batchId, remote);
    await initial.submit(batchId, local);
    await initial.finish(batchId);

    connected = false;
    fixture.archived.add(local.id);
    time = 2000;
    const checked = await fixture.create().check(batchId);
    assert.deepEqual(checked.items[0], pending);
    assert.equal(checked.items[1]?.status, "verified");
    assert.equal(checked.items[1]?.checkedAt, 2);
    assert.deepEqual(fixture.checked.slice(-2), [remote.id, local.id]);

    connected = true;
    fixture.archived.add(remote.id);
    time = 3000;
    const recovered = await fixture.create().check(batchId);
    assert.equal(recovered.items[0]?.status, "verified");
    assert.equal(recovered.items[0]?.checkedAt, 3);
    assert.deepEqual(recovered.items[1], checked.items[1]);
    assert.equal(fixture.writes(), 2);
});

test("a native archive check still stops on an account authority failure", async () => {
    const failure = new Error("Codex 账号已变更");
    let authorized = true;
    const fixture = harness(undefined, undefined, () => {
        if (!authorized) throw failure;
    });
    const batchId = crypto.randomUUID();
    const archive = fixture.create();
    const first = item("pending-first");
    const second = item("pending-second");
    await archive.start(batchId, [first, second]);
    await archive.submit(batchId, first);
    await archive.submit(batchId, second);
    authorized = false;
    await assert.rejects(fixture.create().check(batchId), (error) => error === failure);
    assert.equal(fixture.checked.length, 3);
    assert.deepEqual(
        archive.batches()[0]?.items.map(({ status }) => status),
        ["pending", "pending"],
    );
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

test("a repeated batch registration accepts only the original ordered targets", async () => {
    const original = harness().create();
    const batchId = crypto.randomUUID();
    const first = item("first");
    const second = item("second");
    const batch = await original.start(batchId, [first, second]);
    assert.deepEqual(await original.start(batchId, [first, second]), batch);
    for (const replacement of [
        { ...first, title: "other selection" },
        { ...first, version: 4 },
        { ...first, inputVersion: 5 },
        { ...first, sectionVersion: "unexpected" },
    ]) {
        await assert.rejects(original.start(batchId, [replacement, second]), /名单已固定/);
    }
    await assert.rejects(original.start(batchId, [second, first]), /名单已固定/);
    assert.deepEqual(original.batches(), [batch]);
});

test("finishing a batch during preflight leaves the item unsubmitted", async () => {
    let completePreflight!: () => void;
    const preflight = new Promise<void>((resolve) => {
        completePreflight = resolve;
    });
    const fixture = harness(() => preflight);
    const original = fixture.create();
    const batchId = crypto.randomUUID();
    const candidate = item("awaiting");
    await original.start(batchId, [candidate]);
    const result = original.submit(batchId, candidate);
    const finished = await original.finish(batchId);
    assert.equal(finished.items[0]?.status, "awaiting");
    completePreflight();
    assert.equal((await result).status, "awaiting");
    assert.deepEqual(await fixture.create().check(batchId), finished);
    assert.deepEqual(fixture.checked, []);
    assert.equal(fixture.writes(), 0);
});

test("a failed preflight cannot change an item after its batch finishes", async () => {
    let failPreflight!: (error: Error) => void;
    const preflight = new Promise<void>((_resolve, reject) => {
        failPreflight = reject;
    });
    const fixture = harness(() => preflight);
    const archive = fixture.create();
    const id = crypto.randomUUID();
    const candidate = item("unchanged");
    await archive.start(id, [candidate]);
    const result = archive.submit(id, candidate);
    await archive.finish(id);
    failPreflight(new ArchivePreconditionError("输入版本已变更"));
    assert.equal((await result).status, "awaiting");
    assert.equal(fixture.writes(), 0);
});

test("an item claimed before its batch finishes still records the native result", async () => {
    let started!: () => void;
    const inFlight = new Promise<void>((resolve) => {
        started = resolve;
    });
    let confirm!: () => void;
    const receipt = new Promise<void>((resolve) => {
        confirm = resolve;
    });
    const fixture = harness(
        async () => {},
        () => {
            started();
            return receipt;
        },
    );
    const archive = fixture.create();
    const id = crypto.randomUUID();
    const candidate = item("claimed");
    await archive.start(id, [candidate]);
    const result = archive.submit(id, candidate);
    await inFlight;
    const finished = await archive.finish(id);
    assert.equal(finished.items[0]?.status, "pending");
    fixture.archived.add(candidate.id);
    confirm();
    assert.equal((await result).status, "archived");
    const stored = await fixture.create().check(id);
    assert.equal(stored.items[0]?.status, "archived");
    assert.equal(stored.finishedAt, finished.finishedAt);
    assert.equal(fixture.writes(), 1);
});
