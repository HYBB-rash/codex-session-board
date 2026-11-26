import assert from "node:assert/strict";
import { readFileSync, readlinkSync } from "node:fs";

const base = "http://127.0.0.1:18767";
const localId = "01a2030f-7097-7723-9469-42555278dff6";
const remoteId = "01a2030f-7145-7ba0-9cb8-0c13b91df59e";
const hostId = "remote-ssh-discovered:macbook";

async function call(path, payload) {
    const init = {};
    if (payload) {
        const token = await call("/api/info");
        init.method = "POST";
        init.headers = { "Content-Type": "application/json", "X-CSRF-Token": token.requestToken };
        init.body = JSON.stringify(payload);
    }
    const response = await fetch(`${base}${path}`, init);
    const value = await response.json();
    if (!response.ok) throw new Error(`${path} ${response.status} ${value.error}`);
    return value;
}

async function ready() {
    for (let attempt = 0; attempt < 40; attempt++) {
        try {
            const snapshot = await call("/api/sessions");
            const remote = snapshot.sessions.find(
                (card) => card.hostId === hostId && card.threadId === remoteId,
            );
            const local = snapshot.sessions.find(
                (card) => card.hostId === "local" && card.threadId === localId,
            );
            if (local && remote && snapshot.health.connected) return { snapshot, local, remote };
        } catch {
            await new Promise((resolve) => setTimeout(resolve, 600));
        }
    }
    throw new Error("尚未读回两张隔离验收专用卡");
}

async function inspection() {
    const { snapshot, local, remote } = await ready();
    assert.equal(local.archived, true);
    const batches = await call("/api/archive-batches");
    const historical = batches.batches.find(
        ({ id }) => id === "4ac26c51-d1b3-494e-9c29-431ae248c999",
    );
    assert.equal(historical?.items[0]?.status, "archived");
    const replies = await Promise.all(
        [local, remote].map(async (card) => {
            const reply = await call("/api/preview-reply", { id: card.id });
            assert.ok(reply.turnId && reply.html);
            return {
                hostId: card.hostId,
                turnId: reply.turnId,
                archive: card.archived,
                noteKept: !!card.note,
            };
        }),
    );
    console.log(
        JSON.stringify({
            phase: "restart",
            replies,
            projectCount: snapshot.projectCatalog.items.length,
        }),
    );
    return { snapshot, local, remote };
}

function confirmTestCopy() {
    const proofs = readFileSync(new URL("../logs/runtime.jsonl", import.meta.url), "utf8")
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line));
    const started = proofs.findLast(({ event }) => event === "loader-entered");
    const expected = new URL("../state/test-app/ChatGPT", import.meta.url).pathname;
    assert.equal(readlinkSync(`/proc/${started.pid}/exe`), expected);
}

async function archiveRemote(card) {
    const expected = {
        id: card.id,
        version: card.version,
        inputVersion: card.inputVersion,
        sectionVersion: card.sectionVersion,
        updatedAt: card.updatedAt,
        ...(card.archiveTurnId !== undefined && { turnId: card.archiveTurnId }),
        attemptId: crypto.randomUUID(),
    };
    const { item } = await call("/api/archive", expected);
    assert.ok(["archived", "verified", "pending"].includes(item.status), item.message);
    for (let attempt = 0; attempt < 48; attempt++) {
        await new Promise((resolve) => setTimeout(resolve, 1000));
        const { remote, snapshot } = await ready();
        if (remote.archived && snapshot.archivedSessions.some(({ id }) => id === remote.id)) {
            assert.equal(remote.note, card.note);
            console.log(
                JSON.stringify({
                    phase: "manual-archive-confirmed",
                    hostId,
                    id: remote.threadId,
                    noteKept: true,
                    readBack: true,
                }),
            );
            return;
        }
    }
    throw new Error("远程手动归档未在检查窗口内读回");
}

confirmTestCopy();
const { remote } = await inspection();
if (process.argv.includes("--archive-fixture")) await archiveRemote(remote);
