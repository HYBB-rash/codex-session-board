import assert from "node:assert/strict";
import { type TestContext, test } from "node:test";
import { emptyLedger } from "./archive.js";
import { BoardService } from "./board-service.js";
import {
    DesktopConnection,
    hostId,
    localHostId,
    type Thread,
    type ThreadId,
    type Turn,
} from "./desktop-connection.js";
import type { CardRecord } from "./records.js";
import { defaults } from "./settings.js";

const remoteName = "remote-ssh-discovered:fixture";
const first = "00000000-0000-4000-8000-000000000011" as ThreadId;
const second = "00000000-0000-4000-8000-000000000012" as ThreadId;

function row(id: ThreadId, fields: Partial<Thread> = {}): Thread {
    return {
        id,
        name: "original",
        preview: "",
        cwd: "/fixture",
        projectId: null,
        updatedAt: 1,
        createdAt: 1,
        recencyAt: null,
        section: null,
        sectionEnteredAt: null,
        ephemeral: false,
        source: "vscode",
        status: { type: "notLoaded" },
        turns: [],
        ...fields,
    };
}

interface ListArgs {
    archived: boolean;
    cursor: string | null;
    limit: number;
    modelProviders: string[];
    sourceKinds: string[];
    threadId: ThreadId;
}

interface NativeFixture {
    hosts: string[];
    list(host: string, args: ListArgs): { data: Thread[]; nextCursor: string | null };
    history?: (id: ThreadId) => Turn[];
}

function readFixture(native: NativeFixture, host: string, method: string, params: ListArgs) {
    switch (method) {
        case "thread/list":
            return native.list(host, params);
        case "thread/turns/list":
            return { data: native.history?.(params.threadId) ?? [], nextCursor: null };
        case "project/list":
            return { data: [], nextCursor: null };
        default:
            return assert.fail(method);
    }
}

function fixture(testContext: TestContext, maxPages = 3) {
    const oldWindow = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    testContext.after(() => Reflect.set(globalThis, "window", oldWindow));
    const records: Record<string, CardRecord> = {};
    const native: NativeFixture = {
        hosts: [localHostId],
        list(_host: string, _args: ListArgs): { data: Thread[]; nextCursor: string | null } {
            return { data: [], nextCursor: null };
        },
    };
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: native.hosts }),
        sidebar: async () => ({ accountId: "account", userId: "user", sections: [] }),
        async send(payload) {
            const packet = payload as {
                hostId: string;
                request: { id: string; method: string; params: ListArgs };
            };
            const { id, method, params } = packet.request;
            const result = readFixture(native, packet.hostId, method, params);
            desktop.receive({
                type: "mcp-response",
                hostId: packet.hostId,
                message: { id, result },
            });
        },
    });
    const service = new BoardService(desktop, {
        load: async () => records,
        save: async ({ id, expected, record }) => {
            assert.ok(native.history);
            assert.equal(records[id]?.version ?? 0, expected);
            records[id] = record;
        },
        settleTitle: async () => assert.fail("Discovery must not rename a card"),
        loadLedger: async () => emptyLedger(),
        writeLedger: async () => assert.fail("Discovery must not write the ledger"),
        settings: async () => ({ ...defaults, maxPages, pageSize: 2, historyPerPoll: 1 }),
        titleEpoch: async () => assert.fail("Titles disabled"),
        closePage: () => assert.fail("Discovery must not navigate"),
        authority: () => {},
        proof: () => {},
        updateStatus: async () => assert.fail("Discovery must not change release"),
    });
    return { native, service, desktop, records };
}

test("discovery pages interactive sources across all providers and keeps the newest duplicate", async (testContext) => {
    const { native, service } = fixture(testContext);
    const requested: (string | null)[] = [];
    native.list = (_host, args) => {
        assert.deepEqual(args.sourceKinds, ["cli", "vscode", "appServer"]);
        assert.deepEqual(args.modelProviders, []);
        assert.equal(args.limit, 2);
        if (args.archived) return { data: [], nextCursor: null };
        requested.push(args.cursor);
        if (args.cursor === null)
            return { data: [row(first, { source: "cli", name: "latest" })], nextCursor: "next" };
        const excluded = row("child" as ThreadId, { source: { subAgent: "review" } });
        return {
            data: [row(first), row(second, { source: "appServer" }), excluded],
            nextCursor: "",
        };
    };
    await service.refresh();
    assert.deepEqual(requested, [null, "next"]);
    const { sessions, health } = service.snapshot();
    assert.equal(health.scanComplete, true);
    assert.deepEqual(
        sessions.map(({ id, title }) => [id, title]),
        [
            [first, "latest"],
            [second, "original"],
        ],
    );
});

test("historical read failures rotate and old interrupted turns do not change native placement", async (testContext) => {
    const { desktop, native, service, records } = fixture(testContext);
    const attempts: ThreadId[] = [];
    const active = "00000000-0000-4000-8000-000000000013" as ThreadId;
    records[second] = { note: "人工备注", manual: true, version: 9, inputVersion: 5 };
    native.list = (_host, { archived }) => ({
        data: archived
            ? []
            : [row(first), row(second), row(active, { status: { type: "active" } })],
        nextCursor: null,
    });
    native.history = (id) => {
        attempts.push(id);
        if (id === first) throw new Error("fixture history unavailable");
        if (id === second) return [{ id: "old", status: "interrupted", completedAt: 2, items: [] }];
        return [];
    };
    await service.refresh();
    assert.deepEqual(attempts, [active, first]);
    const byId = (id: ThreadId) => service.snapshot().sessions.find((card) => card.id === id);
    assert.match(byId(first)?.observationError ?? "", /history unavailable/);
    assert.equal(service.snapshot().health.runtimeVerified, true);
    assert.match(service.snapshot().health.historyError ?? "", /历史暂不可读/);
    await service.refresh();
    assert.deepEqual(attempts, [active, first, active, second]);
    assert.equal(byId(second)?.manual, true);
    assert.equal(byId(second)?.state, null);
    assert.equal(byId(second)?.note, "人工备注");
    assert.equal(byId(second)?.version, 9);
    assert.equal(records[first], undefined);
    desktop.receive({
        type: "codex-app-server-connection-changed",
        hostId: localHostId,
        state: "disconnected",
    });
    await service.refresh();
    assert.equal(byId(second)?.readOnly, true);
    assert.equal(service.snapshot().health.runtimeVerified, false);
});

test("an old final reply suggests review without issuing a move or changing the current state", async (testContext) => {
    const { native, service, records } = fixture(testContext);
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.history = () => [
        {
            id: "old-answer",
            status: "completed",
            completedAt: 1,
            items: [{ id: "done", type: "agentMessage", phase: "final_answer", text: "完成" }],
        },
    ];
    await service.refresh();
    const card = service.snapshot().sessions[0];
    assert.equal(card?.state, null);
    assert.equal(card?.manual, false);
    assert.equal(card?.previewTurnId, "old-answer");
    assert.match(card?.observationWarning ?? "", /可归入待审阅/);
    assert.equal(records[first]?.phaseKey, undefined);
    await service.refresh();
    assert.equal(service.snapshot().sessions[0]?.version, card?.version);
});

test("an incomplete discovery retains partial discoveries and leaves missing cards read-only", async (testContext) => {
    const { native, service, records } = fixture(testContext, 1);
    records[first] = { note: "保留备注", version: 4, inputVersion: 3 };
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    await service.refresh();
    native.list = (_host, { archived }) =>
        archived ? { data: [], nextCursor: null } : { data: [row(second)], nextCursor: "more" };
    await service.refresh();
    const { sessions, health } = service.snapshot();
    assert.equal(health.connected, true);
    assert.equal(health.scanComplete, false);
    assert.equal(health.runtimeVerified, false);
    assert.match(health.error ?? "", /分页上限/);
    const missing = sessions.find(({ id }) => id === first);
    const discovered = sessions.find(({ id }) => id === second);
    assert.equal(discovered?.readOnly, false);
    assert.equal(missing?.note, "保留备注");
    assert.equal(missing?.version, 4);
    assert.equal(missing?.presence, "cachedUnknown");
    assert.equal(missing?.readOnly, true);
    assert.equal(missing?.archived, false);
    await assert.rejects(service.request("/api/open", { id: first }), /只显示缓存/);
});

test("remote cache survives removal, a failed scan, reconnect, and a confirmed archive", async (testContext) => {
    const { native, service, desktop } = fixture(testContext);
    const remote = hostId(remoteName);
    assert.ok(remote);
    const remoteCard = () => service.snapshot().sessions.find(({ hostId }) => hostId === remote);
    let archived = false;
    let broken = false;
    native.hosts.push(remoteName);
    native.list = (host, args) => {
        if (host === remote && broken) throw new Error("list unavailable");
        return {
            data: host === remote && args.archived === archived ? [row(first)] : [],
            nextCursor: null,
        };
    };
    await service.refresh();
    const old = desktop.capture(remote);
    assert.equal(remoteCard()?.readOnly, false);
    native.hosts.pop();
    await service.refresh();
    assert.equal(remoteCard()?.readOnly, true);
    assert.equal(remoteCard()?.archived, false);
    native.hosts.push(remoteName);
    broken = true;
    await service.refresh();
    assert.equal(remoteCard()?.presence, "cachedUnknown");
    assert.throws(() => desktop.assertCurrent(old), /连接已变更/);
    broken = false;
    await service.refresh();
    assert.equal(remoteCard()?.readOnly, false);
    archived = true;
    await service.refresh();
    assert.equal(remoteCard()?.archived, true);
    assert.equal(remoteCard()?.canOpen, true);
    assert.equal(remoteCard()?.readOnly, true);
});
