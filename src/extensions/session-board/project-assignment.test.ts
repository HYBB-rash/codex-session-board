import assert from "node:assert/strict";
import { test } from "node:test";
import { emptyLedger } from "./archive.js";
import { BoardService } from "./board-service.js";
import {
    cardId,
    DesktopConnection,
    hostId,
    localHostId,
    type ThreadId,
} from "./desktop-connection.js";
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

function transportFixture() {
    const thread = initialThread();
    const writes: string[] = [];
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId, remote] }),
        sidebar: async () => ({ accountId: "account", userId: "user", sections: [] }),
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
        load: async () => ({}),
        save: async () => assert.fail("Assignment must not change card records"),
        loadLedger: async () => emptyLedger(),
        writeLedger: async () => assert.fail("Assignment must not change archive data"),
        settings: async () => defaults,
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
