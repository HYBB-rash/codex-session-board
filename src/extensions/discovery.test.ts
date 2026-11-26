import assert from "node:assert/strict";
import { link, mkdir, mkdtemp, readdir, readFile, rm, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { type TestContext, test } from "node:test";
import { type BoardLedger, emptyLedger } from "./archive.js";
import {
    createDesktopConnection,
    hostId,
    localHostId,
    type SidebarContext,
    type Thread,
    type ThreadId,
    type Turn,
} from "./desktop/connection.js";
import type { DesktopCommand, DesktopRequest } from "./desktop/world.js";
import type { CardRecord } from "./records.js";
import { createBoardService } from "./service.js";
import { defaults, readSettings } from "./settings.js";

const remoteName = "remote-ssh-discovered:fixture";
const independentHost = "remote-ssh-discovered:independent";
const controlHost = "remote-control:fixture";
const wslHost = "remote-wsl:fixture";
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
    remoteHostNames?: SidebarContext["remoteHostNames"];
    sections?: SidebarContext["sections"];
    archive?: (id: ThreadId, host: string) => void | Promise<void>;
    unarchive?: (id: ThreadId, host: string) => void;
    move?: (command: Extract<DesktopCommand, { kind: "move" }>) => void;
    list(host: string, args: ListArgs): { data: Thread[]; nextCursor: string | null };
    history?: (id: ThreadId) => Turn[];
    read?: (host: string, id: ThreadId) => Thread | Promise<Thread>;
    open?: (command: Extract<DesktopCommand, { kind: "open" }>) => void;
    closePage?: () => void;
    dataHome?: (host: string) => string;
    files?: (method: string, params: object) => Promise<object>;
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
            await native.archive(params.threadId, host);
            return {};
        case "thread/unarchive":
            assert.ok(native.unarchive);
            native.unarchive(params.threadId, host);
            return {};
        case "project/list":
            return { data: [], nextCursor: null };
        default:
            if (native.files) return native.files(method, params);
            return assert.fail(method);
    }
}

function discoverySidebar(native: NativeFixture, command: DesktopRequest) {
    if (command.kind === "context")
        return {
            accountId: native.accountId ?? "account",
            userId: "user",
            sections: native.sections ?? [],
            ...(native.remoteHostNames ? { remoteHostNames: native.remoteHostNames } : {}),
        };
    if (command.kind === "data-home") return native.dataHome?.(command.hostId);
    if (command.kind === "move" && native.move) return native.move(command);
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
    const settings: typeof defaults = { ...defaults, maxPages, pageSize: 2, historyPerPoll: 1 };
    const native: NativeFixture = {
        hosts: [localHostId],
        list: () => ({ data: [], nextCursor: null }),
    };
    const desktop = createDesktopConnection({
        bootstrap: () => ({ catalogHostIds: native.hosts }),
        sidebar: async (command) => discoverySidebar(native, command),
        async send(payload) {
            const packet = payload as {
                hostId: string;
                request: { id: string; method: string; params: ListArgs };
            };
            const { id, method, params } = packet.request;
            try {
                const result = await readFixture(native, packet.hostId, method, params);
                desktop.receive({
                    type: "mcp-response",
                    hostId: packet.hostId,
                    message: { id, result },
                });
            } catch (error) {
                desktop.receive({
                    type: "mcp-response",
                    hostId: packet.hostId,
                    message: {
                        id,
                        error: {
                            message: (error as Error).message,
                            code: (error as { code?: number }).code,
                        },
                    },
                });
            }
        },
    });
    const service = createBoardService(desktop, {
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
            assert.ok(native.archive || native.move, "Discovery must not write the ledger");
            ledgers.set(account, ledger);
        },
        settings: async () => ({ ...settings }),
        titleEpoch: async () => assert.fail("Titles disabled"),
        closePage: () => (native.closePage ?? (() => assert.ok(native.open)))(),
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

async function taskFixtureCommand(command: string[]) {
    if (command[0] === "/bin/sh" && command[4]) {
        await mkdir(command[4], { recursive: true });
        return { exitCode: 0 };
    }
    if (command[0] === "/bin/ln" && command[1] && command[2]) {
        assert.equal(dirname(command[1]), dirname(command[2]));
        try {
            await link(command[1], command[2]);
            return { exitCode: 0 };
        } catch {
            return { exitCode: 1 };
        }
    }
    assert.fail("Unexpected fixture command");
}

async function taskFilesFixture(testContext: TestContext, native: NativeFixture) {
    const directory = await mkdtemp(join(tmpdir(), "enhancer-task-records-"));
    testContext.after(() => rm(directory, { recursive: true, force: true }));
    native.files = async (method, params) => {
        const { path, command, dataBase64 } = params as {
            path?: string;
            command?: string[];
            dataBase64?: string;
        };
        if (command) return taskFixtureCommand(command);
        if (method === "fs/watch" || method === "fs/unwatch") return {};
        assert.ok(path?.startsWith(`${directory}/`));
        assert.ok(path);
        if (method === "fs/readDirectory")
            return {
                entries: (await readdir(path, { withFileTypes: true })).map((entry) => ({
                    fileName: entry.name,
                    isFile: entry.isFile(),
                })),
            };
        if (method === "fs/readFile") return { dataBase64: await readFile(path, "base64") };
        if (method === "fs/writeFile" && dataBase64) await writeFile(path, dataBase64, "base64");
        else if (method === "fs/remove") await unlink(path);
        else assert.fail(method);
        return {};
    };
    native.dataHome = (host) => join(directory, host === independentHost ? "other" : "one");
    return directory;
}

test("host tasks share confirmed status across aliases and isolate equal IDs on other hosts", async (testContext) => {
    const { native, service, settings, desktop } = fixture(testContext);
    const directory = await taskFilesFixture(testContext, native);
    await Promise.all(["one", "other"].map((name) => mkdir(join(directory, name))));
    native.hosts.push(remoteName, independentHost);
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.history = () => [];
    native.read = () => row(first);
    settings.taskState = "host";
    await service.refresh();
    const [local, remote] = ["local", remoteName].map((id) =>
        service.snapshot().sessions.find(({ hostId }) => hostId === id),
    );
    assert.ok(local && remote && !local.readOnly && !remote.readOnly);
    const edit = ({ id, version, inputVersion, sectionVersion }: typeof local, reason: string) => ({
        id,
        version,
        inputVersion,
        sectionVersion,
        state: "deferred" as const,
        note: reason,
        deferReason: reason,
    });
    const contested = await Promise.allSettled([
        service.update(edit(local, "local edit")),
        service.update(edit(remote, "remote edit")),
    ]);
    assert.deepEqual(contested.map(({ status }) => status).sort(), ["fulfilled", "rejected"]);
    await service.refresh();
    const after = service.snapshot().sessions;
    assert.equal(after.find(({ hostId }) => hostId === independentHost)?.state, null);
    const updated = after.filter(({ hostId }) => hostId !== independentHost);
    assert.equal(updated.length, 2);
    assert.ok(updated.every(({ state }) => state === "deferred"));
    assert.equal(updated[0]?.deferReason, updated[1]?.deferReason);
    desktop.receive({
        type: "codex-app-server-connection-changed",
        hostId: remoteName,
        state: "disconnected",
    });
    await service.refresh();
    const cached = service.snapshot().sessions.find(({ hostId }) => hostId === remoteName);
    assert.equal(cached?.state, "deferred");
    assert.equal(cached?.readOnly, true);
    assert.equal(
        service.snapshot().sessions.find(({ hostId }) => hostId === "local")?.readOnly,
        false,
    );
});

test("only local and SSH hosts participate in the board and task migration", async (testContext) => {
    const { native, service, settings, desktop } = fixture(testContext);
    const directory = await taskFilesFixture(testContext, native);
    await mkdir(join(directory, "one"));
    native.hosts.push(remoteName, controlHost, wslHost);
    native.remoteHostNames = { [controlHost]: "MacBook Remote Control" };
    const scanned = new Set<string>();
    native.list = (host, { archived }) => {
        scanned.add(host);
        return { data: archived ? [] : [row(first)], nextCursor: null };
    };
    settings.taskState = "migration";
    await service.refresh();
    const view = service.snapshot();
    assert.deepEqual([...scanned].sort(), [localHostId, remoteName]);
    assert.deepEqual(
        view.sessions.map(({ hostId }) => hostId),
        [localHostId, remoteName],
    );
    assert.deepEqual(
        view.projectCatalog.remoteHosts.map(({ hostId }) => hostId),
        [remoteName],
    );
    const [control, wsl] = view.health.unsupportedRemoteHosts;
    assert.equal(control?.hostLabel, "MacBook Remote Control");
    assert.match(control?.reason ?? "", /Remote Control.*暂不支持|暂不支持.*Remote Control/);
    assert.match(wsl?.reason ?? "", /仅支持.*SSH/);
    const preview = (await service.request("/api/task-migration/preview")) as {
        hosts: { hostId: string }[];
    };
    assert.deepEqual(
        preview.hosts.map(({ hostId }) => hostId),
        [localHostId, remoteName],
    );
    const id = JSON.stringify([controlHost, first]);
    await assert.rejects(service.request("/api/open", { id }), /Remote Control.*SSH/);
    await assert.rejects(
        service.request("/api/connect", { hostId: controlHost }),
        /Remote Control.*SSH/,
    );
    await assert.rejects(
        service.request("/api/new", { hostId: controlHost }),
        /Remote Control.*SSH/,
    );
    const controlId = hostId(controlHost);
    assert.ok(controlId);
    assert.throws(() => desktop.capture(controlId), /Remote Control.*SSH/);
});

test("host task migration previews and preserves existing shared records", async (testContext) => {
    const { native, service, settings, records } = fixture(testContext);
    const directory = await taskFilesFixture(testContext, native);
    await mkdir(join(directory, "one"));
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [row(first)],
        nextCursor: null,
    });
    native.history = () => [];
    native.read = () => row(first);
    records[first] = { note: "Original note", version: 8, inputVersion: 3 };
    settings.taskState = "migration";
    const preview = (await service.request("/api/task-migration/preview")) as {
        token: string;
        hosts: { candidates: { id: string; action: string }[] }[];
    };
    assert.equal(preview.hosts[0]?.candidates[0]?.action, "import");
    assert.equal(service.snapshot().sessions[0]?.note, "");
    assert.equal(service.snapshot().sessions[0]?.readOnly, true);
    assert.deepEqual(
        await service.request("/api/task-migration/apply", { confirm: preview.token }),
        { token: preview.token, imported: [first], pending: [] },
    );
    assert.equal(service.snapshot().sessions[0]?.note, "Original note");
    await assert.rejects(
        service.request("/api/task-migration/apply", { confirm: preview.token }),
        /迁移清单已变更/,
    );
    records[first] = { note: "Alternate old note", version: 9, inputVersion: 3 };
    const again = (await service.request("/api/task-migration/preview")) as typeof preview;
    assert.equal(again.hosts[0]?.candidates[0]?.action, "conflict");
    assert.deepEqual(await service.request("/api/task-migration/apply", { confirm: again.token }), {
        token: again.token,
        imported: [],
        pending: [],
    });
    assert.equal(service.snapshot().sessions[0]?.note, "Original note");
});

test("a late host phase cannot undo a manual deferral made on that turn", async (testContext) => {
    const { native, service, settings } = fixture(testContext);
    const directory = await taskFilesFixture(testContext, native);
    await mkdir(join(directory, "one"));
    settings.taskState = "host";
    const startedAt = Date.now() / 1000;
    let session = row(first, { status: { type: "active" }, updatedAt: startedAt });
    let turn: Turn = { id: "turn-one", status: "inProgress", startedAt, items: [] };
    native.read = () => session;
    native.list = (_host, { archived }) => ({
        data: archived ? [] : [session],
        nextCursor: null,
    });
    native.history = () => [turn];
    await service.refresh();
    const running = service.snapshot().sessions[0];
    assert.equal(running?.state, "in_progress");
    assert.ok(running);
    await service.update({
        id: running.id,
        version: running.version,
        inputVersion: running.inputVersion,
        sectionVersion: running.sectionVersion,
        state: "deferred",
        deferReason: "Wait for the owner",
    });
    turn = { ...turn, status: "completed", completedAt: startedAt + 30 };
    session = row(first, { status: { type: "idle" }, updatedAt: startedAt + 30 });
    service.notice(localHostId, first);
    await service.refresh();
    assert.equal(service.snapshot().sessions[0]?.state, "deferred");
    turn = { id: "turn-two", status: "inProgress", startedAt: startedAt + 60, items: [] };
    session = row(first, { status: { type: "active" }, updatedAt: startedAt + 60 });
    service.notice(localHostId, first);
    await service.refresh();
    assert.equal(service.snapshot().sessions[0]?.state, "in_progress");
});

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

    const { id, version, inputVersion, sectionVersion, updatedAt } = old;
    const revisions = { id, version, inputVersion, sectionVersion };
    const archiveRequest = { ...revisions, attemptId: crypto.randomUUID(), updatedAt };
    await assert.rejects(
        service.request("/api/state", { ...revisions, state: null, note: "旧账号草稿" }),
        /卡片位置或输入已更新/,
    );
    await assert.rejects(service.request("/api/archive", archiveRequest), /卡片位置或输入已更新/);
    await assert.rejects(
        service.request("/api/archive-batches", {
            id: crypto.randomUUID(),
            items: [revisions],
        }),
        /归档请求不完整/,
    );
    await assert.rejects(
        service.request("/api/archive", { ...archiveRequest, sectionVersion: "[null,null,null]" }),
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
    native.unarchive = (id) => {
        archivedThreads.delete(id);
    };
    return { archivedThreads, writes, sectionVersion: JSON.stringify([replica, section, 10]) };
}

function archiveClick(
    card: ReturnType<ReturnType<typeof createBoardService>["snapshot"]>["sessions"][number],
) {
    const { id, updatedAt, version, inputVersion, sectionVersion, archiveTurnId, archiveInputKey } =
        card;
    return {
        id,
        updatedAt,
        version,
        inputVersion,
        sectionVersion,
        turnId: archiveTurnId,
        inputKey: archiveInputKey,
        attemptId: crypto.randomUUID(),
    };
}

test("manual archive blocks waiting and stale turns and allows an explicit retry after a proven rejection", async (testContext) => {
    const { native, service } = fixture(testContext);
    const { archivedThreads, writes } = archiveFixture(native);
    const before = row(second, { status: { type: "active", activeFlags: ["waitingOnApproval"] } });
    let latest: Turn = { id: "turn-a", status: "completed", items: [] };
    native.read = (_host, id) => (id === second ? before : row(id));
    native.history = () => [latest];
    native.list = (_host, { archived }) => ({
        data: [first, second]
            .filter((id) => archivedThreads.has(id) === archived)
            .map((id) => (id === second ? before : row(id))),
        nextCursor: null,
    });
    const current = () => {
        const result = service.snapshot().sessions.find(({ id }) => id === second);
        assert.ok(result);
        return result;
    };
    await service.refresh();
    assert.match(current().archiveReason ?? "", /等待批准/);
    const blocked = (await service.request("/api/archive", archiveClick(current()))) as {
        item: { status: string };
    };
    assert.equal(blocked.item.status, "rejected");
    before.status = { type: "idle" };
    await service.refresh();
    const stale = archiveClick(current());
    latest = { ...latest, id: "turn-b" };
    const newTurn = (await service.request("/api/archive", stale)) as { item: { status: string } };
    assert.equal(newTurn.item.status, "newInput");
    assert.deepEqual(writes, []);
    await service.refresh();

    const original = native.archive;
    assert.ok(original);
    native.archive = () => {
        throw Object.assign(new Error("no rollout found"), { code: -32600 });
    };
    const denied = (await service.request("/api/archive", archiveClick(current()))) as {
        item: { status: string };
    };
    assert.equal(denied.item.status, "rejected");
    assert.equal(current().archivePending, false);
    native.archive = original;
    const shown = archiveClick(current());
    before.updatedAt += 1;
    await service.request("/api/archive", shown);
    assert.deepEqual(writes, [second]);
    assert.deepEqual(
        service.snapshot().archivedSessions.map(({ id }) => id),
        [second],
    );
});

test("an unconfirmed archive stays in the board until late native readback and never sends again", async (testContext) => {
    const { native, service } = fixture(testContext);
    const { archivedThreads } = archiveFixture(native);
    let writes = 0;
    native.archive = () => {
        writes++;
        throw new Error("ack lost");
    };
    await service.refresh();
    const card = service.snapshot().sessions.find(({ id }) => id === second);
    assert.ok(card);
    const result = (await service.request("/api/archive", archiveClick(card))) as {
        item: { status: string };
    };
    assert.equal(result.item.status, "pending");
    assert.equal(
        service.snapshot().sessions.find(({ id }) => id === second)?.archiveChecking,
        true,
    );
    await service.refresh();
    await assert.rejects(service.request("/api/archive", archiveClick(card)), /已有操作待核对/);
    assert.equal(writes, 1);
    archivedThreads.add(second);
    await service.refresh();
    const history = service.snapshot().archivedSessions;
    assert.deepEqual(
        history.map(({ id }) => id),
        [second],
    );
    assert.equal(history[0]?.archivedAt, undefined);
    assert.ok(history[0]?.checkedAt);
    assert.equal(writes, 1);
});

test("host-owned archive claims permit a fresh archive of the same restored session", async (testContext) => {
    const { native, service, settings } = fixture(testContext);
    const directory = await taskFilesFixture(testContext, native);
    await mkdir(join(directory, "one"));
    settings.taskState = "host";
    const { writes } = archiveFixture(native);
    native.sections?.push({
        id: "review" as SidebarContext["sections"][number]["id"],
        name: "待审阅",
        hostSectionIds: { local: "review-replica" as NonNullable<Thread["section"]>["id"] },
        itemKeys: [],
    });
    const read = native.read;
    assert.ok(read);
    let moved = false;
    function placementAfterRestore(thread: Thread) {
        if (!moved) return thread;
        return {
            ...thread,
            section: {
                id: "review-replica" as NonNullable<Thread["section"]>["id"],
                name: "待审阅",
            },
        };
    }
    native.move = () => {
        moved = true;
    };
    native.read = async (host, id) => placementAfterRestore(await read(host, id));
    await service.refresh();
    const card = () => {
        const current = service.snapshot().sessions.find(({ id }) => id === second);
        assert.ok(current);
        return current;
    };
    const initial = card();
    assert.equal(initial.canArchive, true);
    await service.request("/api/archive", archiveClick(initial));
    assert.equal(card().archived, true);
    assert.deepEqual(await service.request("/api/archive-restore", { id: second }), {
        restored: true,
    });
    await service.refresh();
    assert.equal(card().archivePending, false);
    assert.equal(card().inputVersion, initial.inputVersion);
    await service.request("/api/archive", archiveClick(card()));
    assert.deepEqual(writes, [second, second]);
    assert.deepEqual(
        service.snapshot().archivedSessions.map(({ id }) => id),
        [second],
    );
});

test("archive migration is read-only in preview and continues once for a reconnected owning host", async (testContext) => {
    const { native, service, desktop, records } = fixture(testContext);
    const local = localHostId;
    const remote = hostId(remoteName);
    assert.ok(remote);
    native.hosts = [local, remote];
    native.sections = ["待归档", "待审阅"].map((name, index) => ({
        id: `section-${index}` as SidebarContext["sections"][number]["id"],
        name,
        itemKeys: [],
        hostSectionIds: {
            [local]: `local-${index}` as NonNullable<Thread["section"]>["id"],
            [remote]: `remote-${index}` as NonNullable<Thread["section"]>["id"],
        },
    }));
    const placed = (host: string, index: number, at: number) => {
        const section = native.sections?.[index];
        assert.ok(section);
        const id = section.hostSectionIds?.[host];
        assert.ok(id);
        return row(first, { section: { id, name: section.name }, sectionEnteredAt: at });
    };
    const sessions = new Map([
        [local, placed(local, 0, 10)],
        [remote, placed(remote, 0, 10)],
    ]);
    native.list = (host, { archived }) => ({
        data: archived ? [] : [sessions.get(host as typeof local) as Thread],
        nextCursor: null,
    });
    native.read = (host) => sessions.get(host as typeof local) as Thread;
    native.history = () => [];
    records[first] = {
        note: "备注",
        version: 3,
        inputVersion: 1,
        manual: true,
        inputKey: "[2]",
        confirmedInputKey: "[1]",
    };
    const moves: string[] = [];
    native.move = ({ hostId: destination, to }) => {
        const target = to === "section-1" ? 1 : 0;
        moves.push(destination);
        sessions.set(destination as typeof local, placed(destination, target, moves.length + 10));
    };
    await service.refresh();
    const connectivity = (state: string) =>
        desktop.receive({
            type: "codex-app-server-connection-changed",
            hostId: remote,
            state,
        });
    connectivity("disconnected");
    const preview = (await service.request("/api/archive-migration/preview")) as {
        token: string;
        hosts: { available: boolean }[];
    };
    assert.deepEqual(moves, []);
    assert.deepEqual(
        preview.hosts.map(({ available }) => available),
        [true, false],
    );
    await service.request("/api/archive-migration/apply", { confirm: preview.token });
    await service.refresh();
    assert.deepEqual(moves, [local]);
    connectivity("connected");
    await service.refresh();
    await service.refresh();
    assert.deepEqual(moves, [local, remote]);
    assert.ok(service.snapshot().sessions.every(({ state }) => state === "review"));
    assert.equal(service.snapshot().sessions.find(({ hostId }) => hostId === local)?.note, "备注");
    assert.equal(
        service.snapshot().sessions.find(({ hostId }) => hostId === local)?.followUp,
        true,
    );
    const roll = (await service.request("/api/archive-migration/preview")) as { token: string };
    sessions.set(remote, placed(remote, 1, 50));
    await service.request("/api/archive-migration/rollback", { confirm: roll.token });
    assert.deepEqual(moves, [local, remote, local]);
    assert.equal(sessions.get(remote)?.sectionEnteredAt, 50);
});

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
    await assert.rejects(service.request("/api/archive-batches", { id, items }), /归档请求不完整/);
    await service.request("/api/archive-batches/check", { id });
    assert.deepEqual(statuses(), ["verified", "awaiting"]);
    assert.deepEqual(writes, []);
    await assert.rejects(
        service.request("/api/archive", { ...target, batchId: id }),
        /归档请求不完整/,
    );
    assert.deepEqual(writes, []);
    assert.deepEqual(statuses(), ["verified", "awaiting"]);

    const current = service.snapshot().sessions.find((card) => card.id === second);
    assert.ok(current?.canArchive);
    assert.notEqual(current.sectionVersion, sectionVersion);
    const newTarget = {
        ...target,
        sectionVersion: current.sectionVersion,
        updatedAt: current.updatedAt,
        attemptId: crypto.randomUUID(),
    };
    await service.request("/api/archive", newTarget);
    await service.request("/api/archive", newTarget);
    assert.deepEqual(writes, [second]);
    assert.deepEqual(statuses(), ["archived"]);
    assert.ok(archivedThreads.has(second));
    assert.deepEqual(
        service
            .snapshot()
            .archivedSessions.map(({ id }) => id)
            .sort(),
        [first, second],
    );
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

test("remote host labels follow the native names without changing host or card identity", async (testContext) => {
    const { native, service } = fixture(testContext);
    const managedHost = "remote-ssh-codex-managed:00000000-0000-4000-8000-000000000021";
    native.hosts = [localHostId, managedHost, remoteName];
    native.remoteHostNames = { [managedHost]: "MacBook Pro" };
    native.list = (host, { archived }) => ({
        data: !archived && host === managedHost ? [row(first)] : [],
        nextCursor: null,
    });

    await service.refresh();
    const snapshot = service.snapshot();
    const card = snapshot.sessions[0];
    assert.equal(card?.hostLabel, "MacBook Pro");
    assert.equal(card?.hostId, managedHost);
    assert.deepEqual(snapshot.projectCatalog.remoteHosts, [
        { hostId: managedHost, hostLabel: "MacBook Pro" },
        { hostId: remoteName, hostLabel: "fixture" },
    ]);

    native.remoteHostNames = { [managedHost]: "Work Mac" };
    await service.refresh();
    const updated = service.snapshot();
    assert.equal(updated.sessions[0]?.id, card?.id);
    assert.equal(updated.sessions[0]?.hostLabel, "Work Mac");
    assert.equal(updated.health.remoteRegistry[0]?.hostLabel, "Work Mac");
});

test("obsolete archive settings never schedule native writes on the local or SSH host", async (testContext) => {
    const { native, service, settings, ledgers } = fixture(testContext);
    native.hosts = [localHostId, remoteName];
    let time = 1;
    testContext.mock.method(Date, "now", () => time * 1000);
    const obsolete = { autoArchiveSeconds: 1, remoteAutoArchiveSeconds: 1 };
    assert.deepEqual(readSettings({ ...settings, ...obsolete }), settings);
    Object.assign(settings, obsolete);
    const { writes } = archiveFixture(native);
    await service.refresh();
    time = 90000;
    await service.refresh();
    assert.deepEqual(writes, []);
    assert.equal(ledgers.size, 0);
    assert.ok(!("to_archive" in service.snapshot().states));
});

test("opening rejects invalid, undiscovered and mismatched threads and uses the verified host", async (testContext) => {
    const { native, service } = fixture(testContext);
    const closePage = testContext.mock.fn();
    native.closePage = closePage;
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
    const command = {
        accountId: "account",
        userId: "user",
        hostId: remote,
        threadId: first,
        kind: "open",
    };
    assert.deepEqual(dispatched, [command]);
    assert.equal(closePage.mock.callCount(), 1);
    assert.deepEqual(await service.openBesideBoard(id), { ok: true });
    assert.deepEqual(dispatched[1], { ...command, besideBoard: true });
    assert.deepEqual(reads, [remote, remote, remote]);
    assert.equal(closePage.mock.callCount(), 1);
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
