import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { setImmediate as nextTick } from "node:timers/promises";
import {
    DesktopConnection,
    type HostSectionId,
    type LogicalSectionId,
    localHostId,
    type SidebarContext,
    type Thread,
    type ThreadId,
} from "./desktop/connection.js";
import { BoardRecords, TitleEpochs } from "./records.js";
import { BoardService } from "./service.js";
import { emptyLedger } from "./session-board/archive.js";
import { defaults } from "./settings.js";

const first = "00000000-0000-4000-8000-000000000001" as ThreadId;
const second = "00000000-0000-4000-8000-000000000002" as ThreadId;
const account = JSON.stringify(["account", "user"]);
const sections = [
    { id: "review", name: "待审阅" },
    { id: "active", name: "进行中" },
    { id: "blocked", name: "已阻塞" },
    { id: "archive", name: "待归档" },
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
    moved: (row: Thread) => void,
) {
    async function handle({ method, params }: Packet["request"]): Promise<unknown> {
        if (method === "thread/list")
            return { data: params.archived ? [] : [...threads.values()], nextCursor: null };
        if (method === "project/list") return { data: [], nextCursor: null };
        const row = params.threadId && threads.get(params.threadId);
        assert.ok(row);
        if (method === "thread/turns/list") return { data: row.turns, nextCursor: null };
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
            moved(row);
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
    const file = join(directory, "records.json");
    const records = new BoardRecords(file);
    const titleEpoch = (key: string) =>
        new TitleEpochs(join(directory, "title-activation.json")).start(key);
    return { records, titleEpoch, file };
}

function assertMoveProof(stage: string): void {
    assert.equal(stage, "board-native-move");
}

function seedTitle(records: BoardRecords, id: ThreadId): void {
    records.write({
        account,
        id,
        expected: 0,
        record: { note: "持久备注", version: 1, inputVersion: 3, title: { state: "review" } },
    });
}

function fixture(testContext: TestContext) {
    const { records, titleEpoch, file } = testRecords(testContext);
    const threads = new Map([first, second].map((id) => [id, thread(id)]));
    const names: { threadId: ThreadId; name: string }[] = [];
    const config = { syncTitles: true };
    const native = {
        preparePhase: records.preparePhase.bind(records),
        context: async () => context,
        async rename(row: Thread, name: string): Promise<void> {
            row.name = name;
        },
        moved: (_row: Thread): void => {},
    };

    function reopen() {
        const store = new BoardRecords(file);
        const desktop = desktopFixture(
            threads,
            () => native.context(),
            async (row, name) => {
                names.push({ threadId: row.id, name });
                await native.rename(row, name);
            },
            (row) => native.moved(row),
        );
        return new BoardService(desktop, {
            load: async (key) => ({ records: store.account(key), legacyCachedCards: [] }),
            save: async (update) => store.write(update),
            preparePhase: async (update) => native.preparePhase(update),
            settleTitle: async (resolution) => store.settleTitle(resolution),
            loadLedger: async () => emptyLedger(),
            writeLedger: async () => assert.fail("Title must not touch the ledger"),
            settings: async () => ({ ...defaults, ...config }),
            titleEpoch: async (key) => titleEpoch(key),
            closePage: () => assert.fail("Title must not navigate"),
            authority: () => {},
            updateStatus: async () => assert.fail("Title must not change release"),
            proof: assertMoveProof,
        });
    }

    const seed = seedTitle.bind(null, records);
    return { reopen, names, threads, native, records, seed, config, titleEpoch };
}

function save(service: BoardService, state: "review" | "in_progress" | "to_archive", note: string) {
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

function conclude(row: Thread, completedAt: number) {
    row.status = { type: "idle" };
    row.updatedAt = completedAt;
    row.section = { id: "active-replica" as HostSectionId, name: "进行中" };
    row.turns = [
        {
            id: `turn-${row.id}`,
            status: "completed",
            startedAt: completedAt - 1,
            completedAt,
            items: [],
        },
    ];
}

test("enabling and reopening do not rename historical activity; new automatic states sync", async (testContext) => {
    const { config, reopen, names, native, threads, titleEpoch } = fixture(testContext);
    let clock = Date.now();
    testContext.mock.method(Date, "now", () => clock);
    config.syncTitles = false;
    const service = reopen();
    await service.refresh();
    const historical = threads.get(first);
    const active = threads.get(second);
    assert.ok(historical && active);
    clock += 10000;
    conclude(historical, clock / 1000 - 2);
    config.syncTitles = true;
    await service.refresh();
    assert.equal(
        service.snapshot().sessions.find(({ threadId }) => threadId === first)?.state,
        "review",
    );
    assert.equal(names.length, 0);
    const activated = titleEpoch(account);
    assert.equal(activated, clock / 1000);
    clock += 10000;
    const reopened = reopen();
    await reopened.refresh();
    assert.equal(titleEpoch(account), activated);
    assert.equal(names.length, 0);
    const renamed = deferred<void>();
    native.rename = async (row, name) => {
        row.name = name;
        renamed.resolve();
    };
    clock += 3000;
    conclude(active, clock / 1000);
    await reopened.refresh();
    await renamed.promise;
    await nextTick();
    assert.equal(names.length, 1);
    assert.equal(names[0]?.threadId, second);
    assert.match(names[0]?.name ?? "", /^\[待审阅\]/);
    assert.equal(
        reopened.snapshot().sessions.find(({ threadId }) => threadId === second)?.titleSyncPending,
        undefined,
    );
});

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

test("a rejected native rename preserves the saved card and is retried after reopening", async (testContext) => {
    const { reopen, names, native, records, threads } = fixture(testContext);
    const original = phaseThread(threads).name;
    const rename = native.rename;
    native.rename = async () => {
        throw new Error("rename rejected before writing");
    };
    const service = reopen();
    await service.refresh();
    const saved = await save(service, "in_progress", "标题失败后保留的备注");
    assert.equal(saved.session.state, "in_progress");
    assert.equal(saved.session.note, "标题失败后保留的备注");
    assert.equal(saved.session.titleSyncPending, true);
    assert.match(saved.session.titleSyncError ?? "", /rejected before writing/);
    assert.equal(phaseThread(threads).name, original);
    const pending = records.account(account)[first];
    assert.equal(pending?.note, saved.session.note);
    assert.equal(pending?.version, saved.session.version);
    assert.equal(pending?.title?.state, "in_progress");
    assert.match(pending?.title?.error ?? "", /rejected before writing/);

    native.rename = rename;
    const restarted = reopen();
    await restarted.refresh();
    const card = phaseCard(restarted);
    assert.equal(names.length, 2);
    assert.deepEqual(names[0], names[1]);
    assert.match(card.title, /^\[进行中\]/);
    assert.equal(card.titleSyncPending, undefined);
    assert.equal(card.state, saved.session.state);
    assert.equal(card.note, saved.session.note);
    assert.equal(card.version, saved.session.version);
    assert.equal(records.account(account)[first]?.title, undefined);
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

function running(row: Thread, id: string, at: number, flags: string[] = []): void {
    row.updatedAt = at;
    row.status = { type: "active", activeFlags: flags };
    row.turns = [{ id, status: "inProgress", startedAt: at, items: [] }];
}

function phaseCard(service: BoardService) {
    const card = service.snapshot().sessions.find(({ threadId }) => threadId === first);
    assert.ok(card);
    return card;
}

function phaseThread(threads: Map<ThreadId, Thread>) {
    const row = threads.get(first);
    assert.ok(row);
    return row;
}

test("automatic pause, resume and completion follow native evidence and preserve the manual note", async (testContext) => {
    const { config, reopen, threads } = fixture(testContext);
    config.syncTitles = false;
    let now = Date.now();
    testContext.mock.method(Date, "now", () => now);
    const service = reopen();
    await service.refresh();
    const row = phaseThread(threads);
    await save(service, "review", "人工备注继续保留");
    now += 3000;
    running(row, "live", now / 1000, ["waitingOnApproval"]);
    const [turn] = row.turns;
    assert.ok(turn);
    await service.refresh();
    const waiting = phaseCard(service);
    assert.equal(waiting.state, "blocked");
    assert.match(waiting.reason ?? "", /批准/);
    await service.refresh();
    assert.equal(phaseCard(service).version, waiting.version);
    row.status = { type: "active", activeFlags: [] };
    turn.status = "completed";
    await service.refresh();
    assert.equal(phaseCard(service).state, "blocked");
    turn.status = "inProgress";
    await service.refresh();
    assert.equal(phaseCard(service).state, "in_progress");
    row.status.activeFlags = ["waitingOnUserInput"];
    await service.refresh();
    assert.equal(phaseCard(service).state, "blocked");
    assert.match(phaseCard(service).reason ?? "", /交互问题/);
    now += 3000;
    row.status = { type: "idle" };
    turn.status = "completed";
    turn.completedAt = now / 1000;
    turn.items.push({
        id: "reply",
        type: "agentMessage",
        phase: "final_answer",
        text: "需要澄清这个要求吗？",
    });
    await service.refresh();
    assert.equal(phaseCard(service).state, "review");
    assert.equal(phaseCard(service).note, "人工备注继续保留");
});

test("a durable pending phase is recovered after restart without repeating an applied move", async (testContext) => {
    const { config, reopen, native, records, threads } = fixture(testContext);
    config.syncTitles = false;
    let now = Date.now();
    testContext.mock.method(Date, "now", () => now);
    const service = reopen();
    await service.refresh();
    const row = phaseThread(threads);
    await save(service, "review", "移动时的备注");
    now += 3000;
    running(row, "lost-receipt", now / 1000);
    let moves = 0;
    native.moved = () => {
        moves++;
        throw new Error("native move dispatched");
    };
    const sidebar = native.context;
    native.context = async () => {
        if (moves === 1) throw new Error("lost follow-up context");
        return sidebar();
    };
    await service.refresh();
    assert.equal(moves, 1);
    assert.equal(row.section?.name, "进行中");
    assert.match(records.account(account)[first]?.pendingPhaseKey ?? "", /lost-receipt/);
    assert.equal(records.account(account)[first]?.note, "移动时的备注");
    now += 10000;
    native.context = sidebar;
    const reopened = reopen();
    await reopened.refresh();
    assert.equal(moves, 1);
    assert.equal(phaseCard(reopened).state, "in_progress");
    assert.equal(phaseCard(reopened).manual, false);
    assert.equal(phaseCard(reopened).note, "移动时的备注");
    assert.equal(records.account(account)[first]?.pendingPhaseKey, undefined);
});

test("passive history and queued text cannot start or finish a turn", async (testContext) => {
    const { config, reopen, threads, native } = fixture(testContext);
    config.syncTitles = false;
    const service = reopen();
    await service.refresh();
    const row = phaseThread(threads);
    await save(service, "to_archive", "人工决定");
    const previous = phaseCard(service);
    native.moved = () => assert.fail("没有真实执行，不得移卡");
    running(row, "queued", Date.now() / 1000);
    row.status = { type: "idle" };
    const [turn] = row.turns;
    assert.ok(turn);
    turn.items.push({ id: "user", type: "userMessage" });
    await service.refresh();
    turn.id = "changed-id";
    turn.items.push({ id: "text", type: "agentMessage", phase: "commentary", text: "已完成" });
    await service.refresh();
    const card = phaseCard(service);
    assert.equal(card.state, "to_archive");
    assert.equal(card.manual, true);
    assert.equal(card.note, "人工决定");
    assert.equal(card.inputVersion, previous.inputVersion + 2);
});

test("first native input advances the same revision before a save or a history scan", async (testContext) => {
    const { config, reopen, threads, records } = fixture(testContext);
    config.syncTitles = false;
    const service = reopen();
    await service.refresh();
    const initial = phaseCard(service);
    const beforeScan = threads.get(first);
    const duringScan = threads.get(second);
    assert.ok(beforeScan && duringScan);
    for (const row of [beforeScan, duringScan]) {
        row.turns = [
            {
                id: "first-turn",
                status: "inProgress",
                startedAt: row.updatedAt,
                items: [{ id: "first-input", type: "userMessage" }],
            },
        ];
    }

    await assert.rejects(save(service, "in_progress", "陈旧表单"), /卡片位置或输入已更新/);
    const refreshed = phaseCard(service);
    assert.equal(refreshed.note, initial.note);
    assert.equal(refreshed.state, initial.state);
    assert.equal(refreshed.inputVersion, initial.inputVersion + 1);
    assert.equal(records.account(account)[first]?.confirmedInputKey, undefined);
    await service.refresh();
    assert.equal(phaseCard(service).inputVersion, refreshed.inputVersion);
    const store = records.account(account);
    assert.equal(store[first]?.inputKey, store[second]?.inputKey);
    assert.equal(store[first]?.inputVersion, store[second]?.inputVersion);
    const saved = await save(service, "review", "确认新输入");
    assert.equal(saved.session.inputVersion, refreshed.inputVersion);
    const record = records.account(account)[first];
    assert.ok(record?.inputKey);
    assert.equal(record.confirmedInputKey, record.inputKey);
});

test("manual placement survives same-turn output and restart until the stage changes", async (testContext) => {
    const { config, reopen, threads } = fixture(testContext);
    config.syncTitles = false;
    let now = Date.now();
    testContext.mock.method(Date, "now", () => now);
    const service = reopen();
    await service.refresh();
    const row = phaseThread(threads);
    now += 3000;
    running(row, "continuing", now / 1000);
    await service.refresh();
    assert.equal(phaseCard(service).state, "in_progress");
    await save(service, "to_archive", "手动优先");
    const previous = phaseCard(service);
    const [turn] = row.turns;
    assert.ok(turn);
    turn.items.push({ id: "text", type: "agentMessage", phase: "commentary", text: "仍在执行" });
    await service.refresh();
    assert.deepEqual(phaseCard(service), previous);
    now += 10000;
    const reopened = reopen();
    await reopened.refresh();
    assert.equal(phaseCard(reopened).state, "to_archive");
    assert.equal(phaseCard(reopened).manual, true);
    row.status.activeFlags = ["waitingOnApproval"];
    await reopened.refresh();
    assert.equal(phaseCard(reopened).state, "blocked");
    assert.equal(phaseCard(reopened).manual, false);
    assert.equal(phaseCard(reopened).note, "手动优先");
});

test("only confirmed failures and interruptions move a running turn to blocked", async (testContext) => {
    const { config, reopen, threads } = fixture(testContext);
    config.syncTitles = false;
    let now = Date.now();
    testContext.mock.method(Date, "now", () => now);
    const service = reopen();
    await service.refresh();
    const row = phaseThread(threads);
    await save(service, "review", "保留备注");
    for (const [outcome, reason] of [
        ["failed", /失败/],
        ["interrupted", /中断/],
    ] as const) {
        now += 3000;
        running(row, outcome, now / 1000);
        await service.refresh();
        const [turn] = row.turns;
        assert.ok(turn);
        turn.items.push({ id: "retry", type: "error" });
        await service.refresh();
        assert.equal(phaseCard(service).state, "in_progress");
        now += 3000;
        row.status = { type: "idle" };
        turn.status = outcome;
        turn.completedAt = now / 1000;
        await service.refresh();
        assert.equal(phaseCard(service).state, "blocked");
        assert.match(phaseCard(service).reason ?? "", reason);
        assert.equal(phaseCard(service).note, "保留备注");
    }
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
