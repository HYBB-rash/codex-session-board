import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { DesktopConnection } from "../dist/extensions/desktop/connection.js";
import { BoardLedgers, BoardRecords } from "../dist/extensions/session-board/records.js";
import { BoardService } from "../dist/extensions/session-board/service.js";
import { defaults } from "../dist/extensions/session-board/settings.js";

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

export class BoardFixture {
    #sections;
    #thread = session("review", 1000);
    #turn = firstTurn();
    #writes = 0;
    #recordsFile;
    #ledgerFile;

    constructor(directory, states) {
        this.#sections = states.map(([state, label]) => ({
            id: section(state),
            name: label,
            hostSectionIds: { [host]: replica(state) },
            itemKeys: [],
        }));
        this.#recordsFile = join(directory, "records.json");
        this.#ledgerFile = join(directory, "ledger.json");
        this.reopen();
    }

    reopen() {
        const records = new BoardRecords(this.#recordsFile);
        const ledgers = new BoardLedgers(this.#ledgerFile);
        const desktop = new DesktopConnection({
            bootstrap: () => ({ catalogHostIds: [host] }),
            send: async (request) => this.transport(request, desktop),
            sidebar: async (command) => this.sidebar(command),
        });
        this.service = new BoardService(desktop, {
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

    transport({ hostId, request }, desktop) {
        assert.equal(hostId, host);
        const page = (data) => ({ data: structuredClone(data), nextCursor: null });
        let result;
        switch (request.method) {
            case "thread/list":
                result = page(request.params.archived ? [] : [this.#thread]);
                break;
            case "project/list":
                result = page([]);
                break;
            case "thread/turns/list":
                assert.equal(request.params.threadId, threadId);
                result = page([this.#turn]);
                break;
            case "thread/read":
                assert.equal(request.params.threadId, threadId);
                result = { thread: structuredClone(this.#thread) };
                break;
            default:
                assert.fail(`模型路径意外发送 ${request.method}`);
        }
        desktop.receive({ type: "mcp-response", hostId, message: { id: request.id, result } });
    }

    sidebar(command) {
        if (command.kind === "context") return { ...identity, sections: this.#sections };
        assert.equal(command.kind, "move");
        assert.equal(command.accountId, identity.accountId);
        assert.equal(command.userId, identity.userId);
        assert.equal(command.hostId, host);
        assert.equal(command.threadId, threadId);
        const current = this.#sections.find(
            ({ hostSectionIds }) => hostSectionIds[host] === this.#thread.section.id,
        );
        assert.equal(command.from, current?.id);
        const target = this.#sections.find(({ id }) => id === command.to);
        assert.ok(target, "目标分区不存在");
        this.#thread.section = { id: target.hostSectionIds[host], name: target.name };
        this.#thread.sectionEnteredAt = Date.now() / 1000;
        this.#writes += 1;
    }

    async step(event, clock) {
        if (event === "restart") {
            this.reopen();
            await this.service.refresh();
        } else if (event === "view") {
            this.service.snapshot();
        } else if (event.startsWith("manual:")) {
            const { id, version, inputVersion, sectionVersion } = this.card();
            const result = await this.service.update({
                id,
                version,
                inputVersion,
                sectionVersion,
                state: event.slice("manual:".length),
                note: "人工备注",
            });
            assert.equal(result.ok, true);
            assert.deepEqual(result.session, this.card());
        } else {
            const { status, turn } = observation(event, clock);
            this.#thread.status = status;
            this.#thread.updatedAt = clock;
            this.#turn = turn;
            this.service.notice(host, threadId);
            await this.service.refresh();
        }
    }

    card() {
        const snapshot = this.service.snapshot();
        assert.equal(snapshot.sessions.length, 1);
        const [card] = snapshot.sessions;
        assert.equal(card.threadId, threadId);
        assert.equal(card.hostId, host);
        return card;
    }

    flow() {
        const { state, manual, note, version } = this.card();
        return { state, manual, note, version };
    }

    persisted() {
        const files = [this.#recordsFile, this.#ledgerFile].map((path) =>
            existsSync(path) ? readFileSync(path, "utf8") : null,
        );
        return { files, writes: this.#writes };
    }
}
