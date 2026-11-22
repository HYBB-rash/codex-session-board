import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { emptyLedger } from "./archive.js";
import { BoardService } from "./board-service.js";
import {
    DesktopConnection,
    type HostSectionId,
    type LogicalSectionId,
    localHostId,
    type SidebarContext,
    type Thread,
    type ThreadId,
} from "./desktop-connection.js";
import { BoardRecords } from "./records.js";
import { defaults } from "./settings.js";

const first = "00000000-0000-4000-8000-000000000001" as ThreadId;
const second = "00000000-0000-4000-8000-000000000002" as ThreadId;
const account = JSON.stringify(["account", "user"]);
const sections = [
    { id: "review", name: "待审阅" },
    { id: "active", name: "进行中" },
].map(({ id, name }) => ({
    id: id as LogicalSectionId,
    name,
    hostSectionIds: { local: `${id}-replica` as HostSectionId },
    itemKeys: [],
}));
const context: SidebarContext = { accountId: "account", userId: "user", sections };

function thread(id: ThreadId): Thread {
    return {
        id,
        name: "[待审阅]【进行中】 ✨ [构建] 原标题中段 [已阻塞] ".concat("完整标题".repeat(90)),
        cwd: "/fixture",
        preview: "short preview",
        projectId: null,
        updatedAt: 1,
        createdAt: 1,
        recencyAt: null,
        section: { id: "review-replica" as HostSectionId, name: "待审阅" },
        sectionEnteredAt: 1,
        ephemeral: false,
        status: { type: "notLoaded" },
        turns: [],
    };
}

interface Packet {
    hostId: string;
    request: {
        id: string;
        method: string;
        params: { archived?: boolean; threadId?: ThreadId; name?: string };
    };
}

function desktopFixture(
    threads: Map<ThreadId, Thread>,
    readContext: () => Promise<SidebarContext>,
    rename: (row: Thread, name: string) => Promise<void>,
) {
    async function handle({ method, params }: Packet["request"]): Promise<unknown> {
        if (method === "thread/list")
            return { data: params.archived ? [] : [...threads.values()], nextCursor: null };
        if (method === "thread/turns/list" || method === "project/list")
            return { data: [], nextCursor: null };
        const row = params.threadId && threads.get(params.threadId);
        assert.ok(row);
        if (method === "thread/read") return { thread: row };
        assert.equal(method, "thread/name/set");
        assert.ok(params.name);
        await rename(row, params.name);
        return {};
    }

    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId] }),
        async sidebar(command) {
            if (command.kind === "context") return readContext();
            assert.equal(command.kind, "move");
            const row = threads.get(command.threadId as ThreadId);
            const section = sections.find(({ id }) => id === command.to);
            assert.ok(row && section);
            row.section = { id: section.hostSectionIds.local, name: section.name };
            row.sectionEnteredAt = Date.now();
            return null;
        },
        async send(payload) {
            const packet = payload as Packet;
            desktop.receive({
                type: "mcp-response",
                hostId: packet.hostId,
                message: {
                    id: packet.request.id,
                    result: structuredClone(await handle(packet.request)),
                },
            });
        },
    });
    return desktop;
}

function testRecords(testContext: TestContext) {
    const directory = mkdtempSync(join(tmpdir(), "session-board-title-"));
    const previous = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    testContext.after(() => {
        Reflect.set(globalThis, "window", previous);
        rmSync(directory, { recursive: true, force: true });
    });
    return new BoardRecords(join(directory, "records.json"));
}

function fixture(testContext: TestContext) {
    const records = testRecords(testContext);
    const threads = new Map([first, second].map((id) => [id, thread(id)]));
    const names: { threadId: ThreadId; name: string }[] = [];
    const native = {
        context: async () => context,
        async rename(row: Thread, name: string): Promise<void> {
            row.name = name;
        },
    };

    function reopen() {
        const desktop = desktopFixture(
            threads,
            () => native.context(),
            async (row, name) => {
                names.push({ threadId: row.id, name });
                await native.rename(row, name);
            },
        );
        return new BoardService(desktop, {
            load: async (key) => records.account(key),
            save: async (update) => records.write(update),
            settleTitle: async (resolution) => records.settleTitle(resolution),
            loadLedger: async () => emptyLedger(),
            writeLedger: async () => assert.fail("Title must not touch the ledger"),
            settings: async () => ({ ...defaults, syncTitles: true }),
            closePage: () => assert.fail("Title must not navigate"),
            authority: () => {},
            updateStatus: async () => assert.fail("Title must not change release"),
            proof: (stage, { confirmed }) => {
                assert.equal(stage, "board-native-move");
                assert.equal(confirmed, true);
            },
        });
    }

    function seed(id: ThreadId): void {
        records.write({
            account,
            id,
            expected: 0,
            record: { note: "持久备注", version: 1, inputVersion: 3, title: { state: "review" } },
        });
    }

    return { reopen, names, threads, native, records, seed };
}

function save(service: BoardService, state: "review" | "in_progress", note: string) {
    const card = service.snapshot().sessions.find(({ threadId }) => threadId === first);
    assert.ok(card);
    const { id, version, inputVersion, sectionVersion } = card;
    return service.update({ id, state, note, version, inputVersion, sectionVersion });
}

function deferred<Value>() {
    let resolve!: (value: Value) => void;
    const promise = new Promise<Value>((complete) => {
        resolve = complete;
    });
    return { promise, resolve };
}

test("manual saves sync the latest full title and strip only leading state prefixes", async (testContext) => {
    const { reopen, names, threads } = fixture(testContext);
    const service = reopen();
    await service.refresh();
    const row = threads.get(first);
    assert.ok(row);
    row.name = `[待审阅]【进行中】 ✨ [构建] 新标题中段 [已阻塞] ${"完整标题".repeat(90)}`;
    const expected = `[待审阅] ✨ [构建] 新标题中段 [已阻塞] ${"完整标题".repeat(90)}`;
    const saved = await save(service, "review", "新备注");
    assert.equal(saved.session.title, expected);
    assert.equal(saved.session.note, "新备注");
    assert.equal(saved.session.version, 1);
    assert.equal(saved.session.titleSyncPending, undefined);
    await save(service, "review", "再次保存");
    assert.deepEqual(names, [{ threadId: first, name: expected }]);
});

test("foreground save supersedes a running title job without waiting for its reply", {
    timeout: 7000,
}, async (testContext) => {
    const { reopen, names, native, records, seed } = fixture(testContext);
    seed(first);
    const inFlight = deferred<void>();
    const reply = deferred<void>();
    native.rename = async (row, name) => {
        if (names.length === 1) {
            inFlight.resolve();
            await reply.promise;
        }
        row.name = name;
    };
    const service = reopen();
    const scan = service.refresh();
    await inFlight.promise;
    try {
        const saved = await save(service, "in_progress", "较新的备注");
        assert.equal(saved.session.state, "in_progress");
        assert.equal(saved.session.titleSyncPending, true);
        assert.equal(names.length, 1);
        assert.equal(records.account(account)[first]?.title?.state, "in_progress");
    } finally {
        reply.resolve();
        await scan;
    }
    assert.equal(records.account(account)[first]?.title?.state, "in_progress");
    await service.refresh();
    assert.equal(names.length, 2);
    assert.match(names[1]?.name ?? "", /^\[进行中\]/);
    const current = records.account(account)[first];
    assert.equal(current?.note, "较新的备注");
    assert.equal(current?.version, 2);
    assert.equal(current?.inputVersion, 3);
    assert.equal(current?.title, undefined);
});

test("restart reconciles a lost rename receipt without repeating an applied write", async (testContext) => {
    const { reopen, names, native, records } = fixture(testContext);
    native.rename = async (row, name) => {
        row.name = name;
        throw new Error("rename applied; response lost");
    };
    const service = reopen();
    await service.refresh();
    const saved = await save(service, "review", "断开前保存的备注");
    assert.equal(saved.session.state, "review");
    assert.equal(saved.session.titleSyncPending, true);
    assert.match(saved.session.titleSyncError ?? "", /response lost/);
    const reopened = reopen();
    await reopened.refresh();
    assert.equal(names.length, 1);
    assert.deepEqual(records.account(account)[first], {
        manual: true,
        note: "断开前保存的备注",
        version: saved.session.version,
        inputVersion: saved.session.inputVersion,
        selectedAt: records.account(account)[first]?.selectedAt,
    });
    assert.equal(reopened.snapshot().sessions[0]?.titleSyncPending, undefined);
});

test("a frequently failing title retries behind other pending cards", async (testContext) => {
    const { reopen, names, native, records, seed } = fixture(testContext);
    seed(first);
    seed(second);
    const now = Date.now();
    let elapsed = 0;
    testContext.mock.method(Date, "now", () => now + elapsed);
    native.rename = async (row, name) => {
        if (row.id === first) throw new Error("first thread is unavailable");
        row.name = name;
    };
    const service = reopen();
    await service.refresh();
    elapsed += 31000;
    await service.refresh();
    assert.deepEqual(
        names.map(({ threadId }) => threadId),
        [first, second],
    );
    assert.ok(records.account(account)[first]?.title?.error);
    assert.equal(records.account(account)[second]?.title, undefined);
});

test("a hanging native context respects the shared three-second title budget", {
    timeout: 6000,
}, async (testContext) => {
    const { reopen, names, native, records } = fixture(testContext);
    const service = reopen();
    await service.refresh();
    const outcome = deferred<SidebarContext>();
    native.context = async () =>
        records.account(account)[first]?.title ? outcome.promise : context;
    const started = performance.now();
    try {
        const saved = await save(service, "review", "状态先提交");
        assert.ok(performance.now() - started < 3500);
        assert.equal(saved.session.titleSyncPending, true);
        assert.match(saved.session.titleSyncError ?? "", /标题同步超时/);
        assert.equal(saved.session.note, "状态先提交");
    } finally {
        outcome.resolve(context);
    }
    await Promise.resolve();
    assert.deepEqual(names, []);
});
