import assert from "node:assert/strict";
import { test } from "node:test";
import {
    cardId,
    DesktopConnection,
    hostId,
    localHostId,
    type ThreadId,
} from "./desktop/connection.js";
import type { DesktopCommand } from "./desktop/world.js";
import { BoardService } from "./service.js";
import { type BoardLedger, emptyLedger } from "./session-board/archive.js";
import { defaults } from "./settings.js";

const remote = hostId("remote-ssh-discovered:fixture");
assert.ok(remote);
const threadId = "00000000-0000-4000-8000-000000000001" as ThreadId;
const cwd = "/fixture/project";
const project = { id: "remote-project", name: "Fixture", roots: [{ path: cwd }] };

interface Packet {
    hostId: string;
    request: { id: string; method: string; params: { archived?: boolean; projectId?: string } };
}

function initialThread() {
    return {
        id: threadId,
        name: "Fixture",
        preview: "",
        cwd,
        projectId: null as string | null,
        updatedAt: 1,
        createdAt: 1,
        recencyAt: null,
        section: null,
        sectionEnteredAt: null,
        ephemeral: false,
        status: { type: "idle" },
        turns: [],
    };
}

function deferred<Value>() {
    let resolve!: (value: Value) => void;
    const promise = new Promise<Value>((complete) => {
        resolve = complete;
    });
    return { promise, resolve };
}

function transportFixture(
    open: (command: Extract<DesktopCommand, { kind: "new" }>) => Promise<unknown> = async () =>
        assert.fail("Assignment must not navigate"),
) {
    const thread = initialThread();
    const writes: string[] = [];
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId, remote] }),
        async sidebar(command) {
            if (command.kind === "new") return open(command);
            assert.equal(command.kind, "context");
            return { accountId: "account", userId: "user", sections: [] };
        },
        async send(payload) {
            const packet = payload as Packet;
            const { method, params } = packet.request;
            const message =
                method === "thread/metadata/update"
                    ? update(packet.hostId, params.projectId)
                    : { result: read(packet.hostId, method, params.archived) };
            desktop.receive({
                type: "mcp-response",
                hostId: packet.hostId,
                message: { id: packet.request.id, ...message },
            });
        },
    });
    function update(host: string, id: string | undefined) {
        assert.equal(host, remote);
        if (!id) return assert.fail("Missing project ID");
        writes.push(id);
        thread.projectId = id;
        return { error: { message: "project write committed but acknowledgment lost" } };
    }
    function read(host: string, method: string, archived: boolean | undefined) {
        const page = (data: unknown[]) => ({ data, nextCursor: null });
        switch (method) {
            case "thread/list":
                return page(host === remote && !archived ? [structuredClone(thread)] : []);
            case "project/list":
                return page([host === remote ? project : { ...project, id: "local-project" }]);
            case "thread/read":
                assert.equal(host, remote);
                return { thread: structuredClone(thread) };
            default:
                return assert.fail(`Unexpected fixture call ${method}`);
        }
    }
    return { desktop, writes };
}

function assignmentFixture() {
    const { desktop, writes } = transportFixture();
    const service = new BoardService(desktop, {
        load: async () => ({ records: {}, legacyCachedCards: [] }),
        save: async () => assert.fail("Assignment must not change card records"),
        settleTitle: async () => assert.fail("Assignment must not change titles"),
        preparePhase: async () => assert.fail("Assignment must not advance a phase"),
        loadLedger: async () => emptyLedger(),
        writeLedger: async () => assert.fail("Assignment must not change archive data"),
        settings: async () => defaults,
        titleEpoch: async () => assert.fail("Assignment must not enable title synchronization"),
        closePage: () => assert.fail("Assignment must not navigate"),
        authority: () => {},
        updateStatus: async () => assert.fail("Assignment must not update the board"),
        proof: () => {},
    });
    return { service, writes };
}

test("project assignment verifies a lost native acknowledgment and rejects the wrong host", async () => {
    const original = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    try {
        const { service, writes } = assignmentFixture();
        await service.refresh();
        const id = cardId({ hostId: remote, threadId });
        const target = service
            .snapshot()
            .projectCatalog.items.find(({ hostId }) => hostId === remote);
        assert.ok(target);
        const assign = (projectId: string) =>
            service.request("/api/project/assign", { id, projectId, confirmed: true });
        await assert.rejects(assign("local-project"), /主机或工作目录不一致/);
        assert.deepEqual(writes, []);
        assert.deepEqual(await assign(target.id), { ok: true });
        assert.deepEqual(await assign(target.id), { ok: true });
        assert.deepEqual(writes, [project.id]);
        assert.equal(
            service.snapshot().sessions.find((card) => card.id === id)?.projectId,
            target.id,
        );
    } finally {
        Reflect.set(globalThis, "window", original);
    }
});

function draftFixture() {
    const gates = [deferred<string>(), deferred<string>()].map((outcome) => ({
        outcome,
        entered: deferred<void>(),
    }));
    const calls: string[] = [];
    const { desktop, writes } = transportFixture(async (command) => {
        assert.equal(command.hostId, remote);
        assert.equal(command.projectId, project.id);
        assert.deepEqual(command.roots, [cwd]);
        const gate = gates[calls.length];
        assert.ok(gate, "Unexpected duplicate navigation");
        calls.push(command.projectId);
        gate.entered.resolve();
        return gate.outcome.promise;
    });
    let ledger: BoardLedger = emptyLedger();
    const stages: string[] = [];
    let closeCount = 0;
    const service = new BoardService(desktop, {
        load: async () => ({ records: {}, legacyCachedCards: [] }),
        save: async () => assert.fail("New input page must not change card records"),
        settleTitle: async () => assert.fail("New input page must not change titles"),
        preparePhase: async () => assert.fail("New input page must not advance a phase"),
        loadLedger: async () => ledger,
        async writeLedger(_account, next) {
            ledger = next;
            const stage = ledger.pendingDrafts?.[0]?.stage;
            assert.ok(stage);
            stages.push(stage);
        },
        settings: async () => defaults,
        titleEpoch: async () => assert.fail("New input page must not enable title synchronization"),
        closePage: () => {
            closeCount++;
        },
        authority: () => {},
        updateStatus: async () => assert.fail("New input page must not update the board"),
        proof: () => {},
    });
    return { service, desktop, stages, gates, calls, writes, closed: () => closeCount };
}

async function verifyOpenedDraftReplay(
    service: BoardService,
    desktop: DesktopConnection,
    projectId: string,
    operationId: string,
) {
    assert.ok(remote);
    const request = (hostId: string) =>
        service.request("/api/new", { projectId, operationId, hostId });
    assert.deepEqual(await request(remote), { ok: true, operationId });
    await assert.rejects(request(localHostId), /连接不可用/);
    desktop.receive({
        type: "codex-app-server-connection-changed",
        hostId: remote,
        state: "disconnected",
    });
    await service.refresh();
    assert.equal(
        service.snapshot().projectCatalog.items.find(({ id }) => id === projectId)?.canCreate,
        false,
    );
    assert.deepEqual(await request(remote), { ok: true, operationId });
    await assert.rejects(request(localHostId), /连接不可用/);
}

test("overlapping requests share one native draft open and can resume after registration", {
    timeout: 5000,
}, async () => {
    const original = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    try {
        const { service, desktop, stages, gates, calls, writes, closed } = draftFixture();
        const [registration, composer] = gates;
        assert.ok(registration && composer);
        await service.refresh();
        const target = service
            .snapshot()
            .projectCatalog.items.find(({ hostId }) => hostId === remote);
        assert.ok(target);
        const operationId = "00000000-0000-4000-8000-000000000025";
        const request = (projectId = target.id, hostId = remote) =>
            service.request("/api/new", { projectId, operationId, hostId });
        await assert.rejects(request(target.id, localHostId), /连接不可用/);
        assert.deepEqual(stages, []);
        const first = request();
        await registration.entered.promise;
        const duplicate = request();
        try {
            await assert.rejects(request("local-project", localHostId), /已用于其他项目/);
            await assert.rejects(request(target.id, localHostId), /连接不可用/);
            assert.deepEqual(calls, [project.id]);
            assert.deepEqual(stages, ["starting"]);
        } finally {
            registration.outcome.resolve("registration");
        }
        for (const outcome of await Promise.all([first, duplicate]))
            assert.deepEqual(outcome, { ok: true, registration: true, operationId });
        const registered = request();
        await composer.entered.promise;
        const retry = request();
        composer.outcome.resolve("composer");
        for (const outcome of await Promise.all([registered, retry]))
            assert.deepEqual(outcome, { ok: true, registration: false, operationId });
        await verifyOpenedDraftReplay(service, desktop, target.id, operationId);
        assert.deepEqual(calls, [project.id, project.id]);
        assert.deepEqual(stages, ["starting", "starting", "opened"]);
        assert.deepEqual(writes, []);
        assert.equal(closed(), 2);
    } finally {
        Reflect.set(globalThis, "window", original);
    }
});
