import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createDesktopConnection } from "../dist/extensions/desktop/connection.js";
import { createBoardLedgers, createBoardRecords } from "../dist/extensions/records.js";
import { createBoardService } from "../dist/extensions/service.js";
import { defaults } from "../dist/extensions/settings.js";

const threadId = "00000000-0000-4000-8000-000000000001";
const host = "local";
const identity = { accountId: "acceptance-account", userId: "acceptance-user" };
const account = JSON.stringify([identity.accountId, identity.userId]);
const section = (state) => `section-${state}`;
const replica = (state) => `local-section-${state}`;

function session(state, clock) {
    return {
        id: threadId,
        name: "验收会话",
        preview: "",
        cwd: "/fixture",
        projectId: null,
        updatedAt: clock,
        createdAt: clock,
        recencyAt: clock,
        section: { id: replica(state), name: state },
        sectionEnteredAt: clock,
        ephemeral: false,
        status: { type: "active" },
        turns: [],
    };
}

const firstTurn = () => ({
    id: "round-one",
    status: "inProgress",
    itemsView: "summary",
    startedAt: 1001,
    items: [{ id: "input-one", type: "userMessage" }],
});

function observation(phase, clock) {
    const turn = firstTurn();
    const statuses = {
        running: { type: "active" },
        approval: { type: "active", activeFlags: ["waitingOnApproval"] },
        input: { type: "active", activeFlags: ["waitingOnUserInput"] },
        error: { type: "systemError" },
        finished: { type: "idle" },
    };
    assert.ok(Object.hasOwn(statuses, phase), `未知阶段 ${phase}`);
    if (phase === "finished") {
        turn.status = "completed";
        turn.completedAt = clock;
    }
    return { status: statuses[phase], turn };
}

export function createBoardFixture(directory, states, hooks = {}) {
    const sections = states.map(([state, label]) => ({
        id: section(state),
        name: label,
        hostSectionIds: { [host]: replica(state) },
        itemKeys: [],
    }));
    const thread = session("review", 1000);
    let turn = firstTurn();
    let writes = 0;
    const recordsFile = join(directory, "records.json");
    const ledgerFile = join(directory, "ledger.json");
    let service;
    const interceptTransport =
        hooks.transport ?? ((packet, desktop, forward) => forward(packet, desktop));
    const interceptSidebar = hooks.sidebar ?? ((command, forward) => forward(command));

    function reopen() {
        const records = createBoardRecords(recordsFile);
        const ledgers = createBoardLedgers(ledgerFile);
        const desktop = createDesktopConnection({
            bootstrap: () => ({ catalogHostIds: [host] }),
            send: async (request) => interceptTransport(request, desktop, transport),
            sidebar: async (command) => interceptSidebar(command, sidebar),
        });
        service = createBoardService(desktop, {
            load: async (key) => ({ records: records.account(key), legacyCachedCards: [] }),
            save: async (update) => records.write(update),
            preparePhase: async (update) => records.preparePhase(update),
            loadLedger: async (key) => ledgers.load(key),
            writeLedger: async (key, ledger) => ledgers.save(key, ledger),
            settings: async () => ({ ...defaults, mode: "legacy" }),
            closePage: () => assert.fail("模型路径意外关闭看板"),
            authority: (key) => assert.equal(key, account),
            updateStatus: async () => assert.fail("模型路径意外尝试更新"),
            proof: () => {},
        });
    }

    function transport({ hostId, request }, desktop) {
        assert.equal(hostId, host);
        const page = (data) => ({ data: structuredClone(data), nextCursor: null });
        let result;
        switch (request.method) {
            case "thread/list":
                result = page(request.params.archived ? [] : [thread]);
                break;
            case "project/list":
                result = page([]);
                break;
            case "thread/turns/list":
                assert.equal(request.params.threadId, threadId);
                result = page([turn]);
                break;
            case "thread/read":
                assert.equal(request.params.threadId, threadId);
                result = { thread: structuredClone(thread) };
                break;
            default:
                assert.fail(`模型路径意外发送 ${request.method}`);
        }
        desktop.receive({ type: "mcp-response", hostId, message: { id: request.id, result } });
    }

    function sidebar(command) {
        if (command.kind === "context") return { ...identity, sections: sections };
        assert.equal(command.kind, "move");
        assert.equal(command.accountId, identity.accountId);
        assert.equal(command.userId, identity.userId);
        assert.equal(command.hostId, host);
        assert.equal(command.threadId, threadId);
        const current = sections.find(
            ({ hostSectionIds }) => hostSectionIds[host] === thread.section.id,
        );
        assert.equal(command.from, current?.id);
        const target = sections.find(({ id }) => id === command.to);
        assert.ok(target, "目标分区不存在");
        thread.section = { id: target.hostSectionIds[host], name: target.name };
        thread.sectionEnteredAt = Date.now() / 1000;
        writes += 1;
    }

    async function step(event, clock) {
        if (event === "restart") {
            reopen();
            await service.refresh();
        } else if (event === "view") {
            service.snapshot();
        } else if (event.startsWith("manual:")) {
            const { id, version, inputVersion, sectionVersion } = card();
            const result = await service.update({
                id,
                version,
                inputVersion,
                sectionVersion,
                state: event.slice("manual:".length),
                note: "人工备注",
            });
            assert.equal(result.ok, true);
            assert.deepEqual(result.session, card());
        } else {
            const { status, turn: observedTurn } = observation(event, clock);
            thread.status = status;
            thread.updatedAt = clock;
            turn = observedTurn;
            service.notice(host, threadId);
            await service.refresh();
        }
    }

    function card() {
        const snapshot = service.snapshot();
        assert.equal(snapshot.sessions.length, 1);
        const [card] = snapshot.sessions;
        assert.equal(card.threadId, threadId);
        assert.equal(card.hostId, host);
        return card;
    }

    function flow() {
        const { state, manual, note, version } = card();
        return { state, manual, note, version };
    }

    function persisted() {
        const files = [recordsFile, ledgerFile].map((path) =>
            existsSync(path) ? readFileSync(path, "utf8") : null,
        );
        return { files, writes: writes };
    }

    reopen();
    return {
        get service() {
            return service;
        },
        reopen,
        step,
        card,
        flow,
        persisted,
    };
}
