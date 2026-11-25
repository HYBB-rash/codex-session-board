import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
    projectedTitle,
    runtimePhase,
    runtimeState,
    turnInput,
} from "../dist/extensions/activity.js";
import { labels, visibleStates } from "../dist/extensions/placement.js";
import { BoardFixture } from "./typescript-fixture.mjs";

const input = JSON.parse(await readFile(fileURLToPath(process.argv[2]), "utf8"));
const results = [];

async function check(id, test) {
    try {
        const detail = await test();
        results.push({ id, status: "pass", ...(detail ? { detail } : {}) });
    } catch (error) {
        results.push({ id, status: "fail", detail: String(error.message ?? error) });
    }
}

await check("state_names_and_display_order", () => {
    assert.deepEqual(Object.entries(labels), input.states);
    assert.deepEqual(Object.entries(visibleStates("legacy")), input.states);
    assert.deepEqual(Object.entries(visibleStates("five")), input.states.slice(0, -1));
    assert.deepEqual(Object.entries(visibleStates("four")), input.states.slice(1, -1));
});

function evaluateRuntime(rows, dated) {
    const disagreements = [];
    for (const [kind, status, activeFlags, expected] of rows) {
        const threadStatus = { type: kind, activeFlags };
        const result = dated
            ? runtimePhase(threadStatus, {
                  id: "round-one",
                  status,
                  items: [],
                  startedAt: 1001,
                  completedAt: 1010,
              })
            : runtimeState(threadStatus, { status });
        if ((result?.state ?? null) !== expected)
            disagreements.push({
                kind,
                status,
                activeFlags,
                expected,
                actual: result?.state ?? null,
            });
        assert.ok(!["completed", "to_archive"].includes(result?.state));
    }
    assert.deepEqual(disagreements, []);
}

await check("runtime_evidence_table_frozen_summary", () => evaluateRuntime(input.runtime, false));
await check("runtime_evidence_table_with_native_identity", () =>
    evaluateRuntime(input.runtime, true),
);

await check("runtime_evidence_without_terminal_confirmation", () => {
    assert.equal(runtimePhase({ type: "notLoaded" }, undefined), null);
    const incomplete = { id: "round-one", status: "completed", items: [] };
    for (const type of ["idle", "notLoaded"]) {
        for (const completedAt of [undefined, 0, NaN, Infinity]) {
            assert.equal(runtimePhase({ type }, { ...incomplete, completedAt }), null);
        }
        assert.equal(runtimePhase({ type }, { ...incomplete, completedAt: 1010 })?.state, "review");
    }
    const active = { id: "round-one", status: "inProgress", items: [] };
    const repeated = runtimePhase({ type: "active", activeFlags: ["waitingOnApproval"] }, active);
    const duplicate = runtimePhase(
        { type: "active", activeFlags: ["waitingOnApproval", "waitingOnApproval"] },
        active,
    );
    assert.deepEqual(duplicate, repeated);
});

await check("title_prefix_replacement_and_idempotence", () => {
    const body = "🧪 [项目] 新正文 [进行中] 结束";
    const original = `[已完成] 【待归档】 [进行中] ${body}`;
    const actual = projectedTitle(original, "review");
    assert.equal(actual, `[待审阅] ${body}`);
    assert.equal(projectedTitle(actual, "review"), actual);
    assert.equal(projectedTitle("", "unknown"), "[待判断] 未命名会话");
    assert.equal(projectedTitle(`[Ops] ${body}`, "blocked"), `[已阻塞] [Ops] ${body}`);
});

await check("title_projection_rejects_an_invalid_state", () => {
    assert.throws(() => projectedTitle("🧪 [项目] 新正文 [进行中] 结束", "invalid"));
});

await check("input_ids_ignore_order_and_assistant_updates", () => {
    const first = { id: "u-1", type: "userMessage" };
    const second = { id: "u-2", type: "userMessage" };
    const turn = { id: "round-one", status: "inProgress", items: [first, second] };
    assert.equal(
        turnInput(turn),
        turnInput({ ...turn, items: [second, { id: "a-1", type: "agentMessage" }, first] }),
    );
    assert.equal(turnInput({ ...turn, itemsView: "notLoaded" }), null);
    assert.notEqual(turnInput({ ...turn, items: [first] }), turnInput(turn));
});

function compare(mismatches, label, expected, actual) {
    try {
        assert.deepEqual(actual, expected);
    } catch {
        mismatches.push({ label, expected, actual });
    }
}

async function replay(trace, directory, setClock) {
    setClock(1000);
    const board = new BoardFixture(directory, input.states);
    const mismatches = [];
    await board.step("running", 1000);
    compare(mismatches, "初始状态", input.initial, board.flow());
    compare(mismatches, "初始输入版本", 1, board.card().inputVersion);
    for (const [index, step] of trace.steps.entries()) {
        const clock = 1000 + 10 * (index + 1);
        setClock(clock);
        const unchanged = step.event === "view" || step.event === "restart";
        const before = unchanged ? board.persisted() : null;
        await board.step(step.event, clock);
        compare(mismatches, step.event, step.expected, board.flow());
        if (unchanged)
            compare(mismatches, `${step.event} 磁盘与原生写入`, before, board.persisted());
    }
    return mismatches;
}

async function replayModel() {
    const now = Date.now;
    globalThis.window = { setTimeout, clearTimeout };
    const directory = await mkdtemp(join(tmpdir(), "typescript-board-model-"));
    const failures = [];
    try {
        for (const [index, trace] of input.traces.entries()) {
            const path = join(directory, String(index));
            const setClock = (clock) => {
                Date.now = () => clock * 1000;
            };
            try {
                const mismatches = await replay(trace, path, setClock);
                if (mismatches.length)
                    failures.push({ path: trace.path, event: trace.event, mismatches });
            } catch (error) {
                failures.push({
                    path: trace.path,
                    event: trace.event,
                    error: String(error.message ?? error),
                });
            }
        }
    } finally {
        Date.now = now;
        delete globalThis.window;
        await rm(directory, { recursive: true, force: true });
    }
    return failures;
}

const replayFailures = await replayModel();
await check("all_abstract_state_edges_match_the_typescript_service", () => {
    const examples = [
        ...new Map(replayFailures.map((failure) => [failure.event, failure])).values(),
    ];
    assert.equal(
        replayFailures.length,
        0,
        `${replayFailures.length}/${input.traces.length} 条边不符。示例 ${JSON.stringify(examples)}`,
    );
    return `${input.traces.length} 条边，${input.actions} 次操作，${input.restarts} 次重开，${input.views} 次查看`;
});

await check("model_view_and_restart_do_not_write", () => {
    const writes = replayFailures.flatMap(({ mismatches }) =>
        (mismatches ?? []).filter(({ label }) => label.endsWith("磁盘与原生写入")),
    );
    assert.equal(
        writes.length,
        0,
        `有查看或重开触发持久写入。示例 ${JSON.stringify(writes.slice(0, 3))}`,
    );
    assert.ok(
        replayFailures.every(({ error }) => error === undefined),
        "存在未完成重放的路径",
    );
    return `检查了 ${input.restarts} 次重开和 ${input.views} 次查看`;
});

process.stdout.write(JSON.stringify(results));
