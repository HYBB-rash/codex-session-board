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
}

function fixture(testContext: TestContext, maxPages = 3) {
    const oldWindow = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    testContext.after(() => Reflect.set(globalThis, "window", oldWindow));
    const records: Record<string, CardRecord> = {};
    const native = {
        hosts: [localHostId as string],
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
            assert.ok(["thread/list", "project/list"].includes(method), method);
            const result =
                method === "thread/list"
                    ? native.list(packet.hostId, params)
                    : { data: [], nextCursor: null };
            desktop.receive({
                type: "mcp-response",
                hostId: packet.hostId,
                message: { id, result },
            });
        },
    });
    const service = new BoardService(desktop, {
        load: async () => records,
        save: async () => assert.fail("Discovery must not edit a card"),
        settleTitle: async () => assert.fail("Discovery must not rename a card"),
        loadLedger: async () => emptyLedger(),
        writeLedger: async () => assert.fail("Discovery must not write the ledger"),
        settings: async () => ({ ...defaults, maxPages, pageSize: 2 }),
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
