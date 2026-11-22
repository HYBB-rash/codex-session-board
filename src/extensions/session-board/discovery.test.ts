import assert from "node:assert/strict";
import { type TestContext, test } from "node:test";
import { type BoardLedger, emptyLedger } from "./archive.js";
import { BoardService } from "./board-service.js";
import {
    DesktopConnection,
    hostId,
    localHostId,
    type SidebarContext,
    type Thread,
    type ThreadId,
    type Turn,
} from "./desktop-connection.js";
import type { DesktopCommand } from "./desktop-world.js";
import type { CardRecord } from "./records.js";
import { defaults } from "./settings.js";

const remoteName = "remote-ssh-discovered:fixture";
const first = "00000000-0000-4000-8000-000000000011" as ThreadId;
const second = "00000000-0000-4000-8000-000000000012" as ThreadId;
const vscode = "00000000-0000-4000-8000-000000000015" as ThreadId;

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
    accountId?: string;
    sections?: SidebarContext["sections"];
    archive?: (id: ThreadId) => void;
    list(host: string, args: ListArgs): { data: Thread[]; nextCursor: string | null };
    history?: (id: ThreadId) => Turn[];
    read?: (host: string, id: ThreadId) => Thread | Promise<Thread>;
    open?: (command: Extract<DesktopCommand, { kind: "open" }>) => void;
}

async function readFixture(native: NativeFixture, host: string, method: string, params: ListArgs) {
    switch (method) {
        case "thread/list":
            return native.list(host, params);
        case "thread/turns/list":
            return { data: native.history?.(params.threadId) ?? [], nextCursor: null };
        case "thread/read":
            assert.ok(native.read);
            return { thread: await native.read(host, params.threadId) };
        case "thread/archive":
            assert.ok(native.archive);
            native.archive(params.threadId);
            return {};
        case "project/list":
            return { data: [], nextCursor: null };
        default:
            return assert.fail(method);
    }
}

function discoverySidebar(native: NativeFixture, command: DesktopCommand) {
    if (command.kind === "context")
        return {
            accountId: native.accountId ?? "account",
            userId: "user",
            sections: native.sections ?? [],
        };
    assert.equal(command.kind, "open");
    assert.ok(native.open);
    return native.open(command);
}

function fixture(testContext: TestContext, maxPages = 3) {
    const oldWindow = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    testContext.after(() => Reflect.set(globalThis, "window", oldWindow));
    const records: Record<string, CardRecord> = {};
    const ledgers = new Map<string, BoardLedger>();
    const settings = { ...defaults, maxPages, pageSize: 2, historyPerPoll: 1 };
    const native: NativeFixture = {
        hosts: [localHostId],
        list: () => ({ data: [], nextCursor: null }),
    };
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: native.hosts }),
        sidebar: async (command) => discoverySidebar(native, command),
        async send(payload) {
            const packet = payload as {
                hostId: string;
                request: { id: string; method: string; params: ListArgs };
            };
            const { id, method, params } = packet.request;
            const result = await readFixture(native, packet.hostId, method, params);
            desktop.receive({
                type: "mcp-response",
                hostId: packet.hostId,
                message: { id, result },
            });
        },
    });
    const service = new BoardService(desktop, {
        load: async () => ({ records, legacyCachedCards: [] }),
        save: async ({ id, expected, record }) => {
            assert.ok(native.history);
            assert.equal(records[id]?.version ?? 0, expected);
            records[id] = record;
        },
        settleTitle: async () => assert.fail("Discovery must not rename a card"),
        preparePhase: async () => assert.fail("History must not prepare a native move"),
        loadLedger: async (account) => ledgers.get(account) ?? emptyLedger(),
        writeLedger: async (account, ledger) => {
            assert.ok(native.archive, "Discovery must not write the ledger");
            ledgers.set(account, ledger);
        },
        settings: async () => ({ ...settings }),
        titleEpoch: async () => assert.fail("Titles disabled"),
        closePage: () => assert.ok(native.open, "Discovery must not navigate"),
        authority: () => {},
        proof: () => {},
        updateStatus: async () => assert.fail("Discovery must not change release"),
    });
    return { native, service, desktop, records, ledgers, settings };
}

function deferred<Value>() {
    let resolve!: (value: Value) => void;
    const promise = new Promise<Value>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}

function pauseRead(native: NativeFixture) {
    const started = deferred<void>();
    const reply = deferred<Thread>();
    native.read = () => {
        started.resolve();
        return reply.promise;
    };
    return { started: started.promise, release: () => reply.resolve(row(first)) };
}

test("an in-flight same-column edit cannot persist after the active Codex account switches", async (testContext) => {
    const { native, service, records } = fixture(testContext);
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.history = () => [];
    await service.refresh();
    const [card] = service.snapshot().sessions;
    assert.ok(card);
    const { id, version, inputVersion, sectionVersion } = card;
    const paused = pauseRead(native);
    const pending = assert.rejects(
        service.update({ id, version, inputVersion, sectionVersion, state: null, note: "旧账号" }),
        /Codex 账号已变更/,
    );
    await paused.started;
    native.accountId = "next-account";
    await service.refresh();
    paused.release();
    await pending;
    assert.equal(records[first], undefined);
    assert.equal(service.snapshot().sessions[0]?.note, "");
    await assert.rejects(
        service.request(
            "/api/state",
            { id, version, inputVersion, sectionVersion, state: null, note: "旧账号" },
            '["account","user"]',
        ),
        /Codex 账号已变更/,
    );
    assert.equal(records[first], undefined);
});

test("a delayed native preview cannot return the previous account's reply after refresh", async (testContext) => {
    const { native, service } = fixture(testContext);
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.history = () => [
        {
            id: "complete",
            status: "completed",
            items: [
                { id: "reply", type: "agentMessage", phase: "final_answer", text: "旧账号回复" },
            ],
        },
    ];
    await service.refresh();
    const paused = pauseRead(native);
    const pending = assert.rejects(
        service.request("/api/preview-reply", { id: first }),
        /Codex 账号已变更/,
    );
    await paused.started;
    native.accountId = "next-account";
    await service.refresh();
    paused.release();
    await pending;
});

test("a stale card cannot save or register an archive after a switch to identical native revisions", async (testContext) => {
    const { native, service, records, ledgers } = fixture(testContext);
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.read = () => row(first);
    native.history = () => [];
    await service.refresh();
    const [old] = service.snapshot().sessions;
    assert.ok(old);
    native.accountId = "next-account";
    await service.refresh();
    const [current] = service.snapshot().sessions;
    assert.ok(current);
    assert.equal(old.id, current.id);
    assert.equal(old.version, current.version);
    assert.equal(old.inputVersion, current.inputVersion);
    assert.notEqual(old.sectionVersion, current.sectionVersion);
    assert.notEqual(old.accountScope, current.accountScope);

    const { id, title, version, inputVersion, sectionVersion } = old;
    const revisions = { id, version, inputVersion, sectionVersion };
    await assert.rejects(
        service.request("/api/state", { ...revisions, state: null, note: "旧账号草稿" }),
        /卡片位置或输入已更新/,
    );
    await assert.rejects(service.request("/api/archive", revisions), /卡片位置或输入已更新/);
    await assert.rejects(
        service.request("/api/archive-batches", {
            id: crypto.randomUUID(),
            items: [{ ...revisions, title }],
        }),
        /卡片位置或输入已更新/,
    );
    await assert.rejects(
        service.request("/api/archive", { ...revisions, sectionVersion: "[null,null,null]" }),
        /卡片位置或输入已更新/,
    );
    assert.equal(records[first], undefined);
    assert.equal(ledgers.size, 0);
    const submitted = { ...revisions, sectionVersion: current.sectionVersion };
    await service.request("/api/state", { ...submitted, state: null, note: "当前账号备注" });
    assert.equal(service.snapshot().sessions[0]?.note, "当前账号备注");
});

function archiveFixture(native: NativeFixture) {
    const replica = "archive-replica" as NonNullable<Thread["section"]>["id"];
    const section = "archive" as SidebarContext["sections"][number]["id"];
    native.sections = [
        { id: section, name: "待归档", hostSectionIds: { local: replica }, itemKeys: [] },
    ];
    const thread = (id: ThreadId) =>
        row(id, { sectionEnteredAt: 10, section: { id: replica, name: "待归档" } });
    const archivedThreads = new Set<ThreadId>([first]);
    const writes: ThreadId[] = [];
    native.list = (_host, { archived }) => ({
        data: [first, second].filter((id) => archived === archivedThreads.has(id)).map(thread),
        nextCursor: null,
    });
    native.read = (_host, id) => thread(id);
    native.history = () => [];
    native.archive = (id) => {
        writes.push(id);
        archivedThreads.add(id);
    };
    return { archivedThreads, writes, sectionVersion: JSON.stringify([replica, section, 10]) };
}

test("a previously registered unscoped archive batch keeps its pending and unsent semantics", async (testContext) => {
    const { native, service, ledgers } = fixture(testContext);
    const { archivedThreads, writes, sectionVersion } = archiveFixture(native);
    const id = crypto.randomUUID();
    const account = '["account","user"]';
    const target = { id: second, title: "original", version: 0, inputVersion: 0, sectionVersion };
    const previous: BoardLedger = {
        ...emptyLedger(),
        batches: [
            {
                id,
                createdAt: 10,
                finishedAt: null,
                items: [
                    { ...target, id: first, status: "pending" },
                    { ...target, status: "awaiting" },
                ],
            },
        ],
    };
    ledgers.set(account, previous);
    const statuses = () => ledgers.get(account)?.batches[0]?.items.map(({ status }) => status);
    await service.refresh();
    const items = [{ ...target, id: first }, target];
    const replay = await service.request("/api/archive-batches", { id, items });
    assert.deepEqual(replay, { batch: previous.batches[0] });
    await service.request("/api/archive-batches/check", { id });
    assert.deepEqual(statuses(), ["verified", "awaiting"]);
    assert.deepEqual(writes, []);
    await service.request("/api/archive", { ...target, batchId: id });
    assert.deepEqual(writes, [second]);
    assert.deepEqual(statuses(), ["verified", "archived"]);

    archivedThreads.delete(second);
    await service.refresh();
    const current = service.snapshot().sessions.find((card) => card.id === second);
    assert.ok(current?.canArchive);
    assert.notEqual(current.sectionVersion, sectionVersion);
    const newTarget = { ...target, sectionVersion: current.sectionVersion };
    const newBatch = crypto.randomUUID();
    await service.request("/api/archive-batches", { id: newBatch, items: [newTarget] });
    assert.deepEqual(statuses(), ["awaiting"]);
    await service.request("/api/archive", { ...newTarget, batchId: newBatch });
    assert.deepEqual(writes, [second, second]);
    assert.deepEqual(statuses(), ["archived"]);
});

test("a reopened batch reconciles uncached submitted cards on connected hosts independently", async (testContext) => {
    const { native, service, ledgers, desktop } = fixture(testContext);
    const { writes, sectionVersion } = archiveFixture(native);
    const remote = JSON.stringify([remoteName, first]);
    const account = '["account","user"]';
    const id = crypto.randomUUID();
    const target = { title: "archive fixture", version: 0, inputVersion: 0, sectionVersion };
    ledgers.set(account, {
        ...emptyLedger(),
        batches: [
            {
                id,
                createdAt: 1,
                finishedAt: 2,
                items: [
                    { ...target, id: remote, status: "pending" },
                    { ...target, id: first, status: "pending" },
                    { ...target, id: second, status: "awaiting" },
                ],
            },
        ],
    });
    let archiveVisible = false;
    const reads: string[] = [];
    native.list = (host, { archived }) => {
        if (archived) reads.push(host);
        return { data: archived && archiveVisible ? [row(first)] : [], nextCursor: null };
    };

    await service.refresh();
    assert.equal(service.snapshot().sessions.length, 0);
    reads.length = 0;
    archiveVisible = true;
    await service.request("/api/archive-batches/check", { id });
    const statuses = () => ledgers.get(account)?.batches[0]?.items.map(({ status }) => status);
    assert.deepEqual(reads, [localHostId]);
    assert.deepEqual(statuses(), ["pending", "verified", "awaiting"]);

    native.hosts.push(remoteName);
    desktop.knownHosts();
    reads.length = 0;
    await service.request("/api/archive-batches/check", { id });
    assert.deepEqual(reads, [remoteName]);
    assert.deepEqual(statuses(), ["verified", "verified", "awaiting"]);
    assert.deepEqual(writes, []);
});

test("archive deadlines follow the current host settings and configured time boundaries", async (testContext) => {
    const { native, service, settings } = fixture(testContext);
    native.hosts = [localHostId, remoteName];
    let time = 3601;
    testContext.mock.method(Date, "now", () => time * 1000);
    settings.autoArchiveSeconds = 1800;
    settings.remoteAutoArchiveSeconds = 900;
    type Schedule = [
        seconds: number | null,
        nextAt: number | undefined,
        lastAt: number | undefined,
    ];
    async function expectSchedules(local: Schedule, remote: Schedule) {
        await service.refresh();
        const { health } = service.snapshot();
        const remoteHealth = health.remoteConnections.find(({ hostId }) => hostId === remoteName);
        assert.ok(remoteHealth);
        const timing = [health, remoteHealth].map(
            ({ autoArchiveSeconds, autoArchiveNextAt, autoArchiveLastRun }) => [
                autoArchiveSeconds,
                autoArchiveNextAt,
                autoArchiveLastRun?.at,
            ],
        );
        assert.deepEqual(timing, [local, remote]);
    }
    await expectSchedules([1800, 5400, undefined], [900, 4500, undefined]);
    time = 4501;
    settings.autoArchiveSeconds = 300;
    await expectSchedules([300, 4800, undefined], [900, 5400, 4501]);
    time = 6301;
    settings.remoteAutoArchiveSeconds = null;
    await expectSchedules([300, 6600, 6301], [null, undefined, 4501]);
    settings.mode = "legacy";
    await expectSchedules([null, undefined, 6301], [null, undefined, 4501]);
    time = 6600;
    settings.mode = "four";
    await expectSchedules([300, 6900, 6301], [null, undefined, 4501]);
});

test("opening rejects invalid, undiscovered and mismatched threads and uses the verified host", async (testContext) => {
    const { native, service } = fixture(testContext);
    const invalid = "../settings" as ThreadId;
    const remote = hostId(remoteName);
    assert.ok(remote);
    native.hosts = [localHostId, remote];
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(invalid), row(first)],
        nextCursor: null,
    });
    const dispatched: DesktopCommand[] = [];
    const reads: string[] = [];
    native.open = (command) => void dispatched.push(command);
    await service.refresh();
    assert.equal(service.snapshot().sessions.length, 2);
    for (const id of [invalid, second, JSON.stringify([remote, invalid])])
        await assert.rejects(service.request("/api/open", { id }), /找不到该会话/);
    const id = JSON.stringify([remote, first]);
    native.read = (host) => {
        reads.push(host);
        return row(second);
    };
    await assert.rejects(service.request("/api/open", { id }), /回执会话不一致/);
    assert.deepEqual(dispatched, []);
    native.read = (host, threadId) => {
        reads.push(host);
        return row(threadId);
    };
    assert.deepEqual(await service.request("/api/open", { id }), { ok: true });
    assert.deepEqual(reads, [remote, remote]);
    assert.deepEqual(dispatched, [
        { accountId: "account", userId: "user", hostId: remote, threadId: first, kind: "open" },
    ]);
});

test("discovery pages interactive sources across all providers and keeps the newest duplicate", async (testContext) => {
    const { native, service } = fixture(testContext);
    const requested: (string | null)[] = [];
    const available = [
        Object.assign(row(first, { source: "cli", name: "latest" }), { modelProvider: "openai" }),
        Object.assign(row(vscode), { modelProvider: "fixture-provider" }),
        Object.assign(row(first, { source: "cli", name: "older" }), { modelProvider: "openai" }),
        Object.assign(row(second, { source: "appServer" }), { modelProvider: "fixture-provider" }),
        row("child" as ThreadId, { source: { subAgent: "review" } }),
    ];
    native.list = (_host, args) => {
        assert.deepEqual(args.sourceKinds, ["cli", "vscode", "appServer"]);
        assert.deepEqual(args.modelProviders, []);
        assert.equal(args.limit, 2);
        if (args.archived) return { data: [], nextCursor: null };
        requested.push(args.cursor);
        const offset = Number(args.cursor ?? "0");
        return {
            data: available.slice(offset, offset + args.limit),
            nextCursor: offset + args.limit < available.length ? String(offset + args.limit) : "",
        };
    };
    await service.refresh();
    assert.deepEqual(requested, [null, "2", "4"]);
    const { sessions, health } = service.snapshot();
    assert.equal(health.scanComplete, true);
    assert.deepEqual(
        sessions.map(({ id, title }) => [id, title]),
        [
            [first, "latest"],
            [vscode, "original"],
            [second, "original"],
        ],
    );
});

test("a repeated discovery cursor preserves read cards and reports the scan as incomplete", async (testContext) => {
    const { native, service } = fixture(testContext);
    const requested: (string | null)[] = [];
    native.list = (_host, { archived, cursor }) => {
        if (archived) return { data: [], nextCursor: null };
        requested.push(cursor);
        return { data: [row(cursor === null ? first : second)], nextCursor: "repeat" };
    };
    await service.refresh();
    const { sessions, health } = service.snapshot();
    assert.deepEqual(requested, [null, "repeat"]);
    assert.deepEqual(
        sessions.map(({ id }) => id),
        [first, second],
    );
    assert.equal(health.scanComplete, false);
    assert.equal(health.runtimeVerified, false);
    assert.match(health.error ?? "", /重复的分页游标/);
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
