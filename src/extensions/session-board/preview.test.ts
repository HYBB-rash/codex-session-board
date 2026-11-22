import assert from "node:assert/strict";
import { type TestContext, test } from "node:test";
import {
    DesktopConnection,
    hostId,
    localHostId,
    type Thread,
    type ThreadId,
    type Turn,
} from "./desktop-connection.js";
import { previewReply } from "./preview.js";

const identifier = "00000000-0000-4000-8000-000000000001" as ThreadId;
const remoteHost = "remote-ssh-discovered:preview";

interface Request {
    method: string;
    params: { threadId: string; cursor?: string | null; itemsView?: string };
}

function fixture(context: TestContext, reply: (request: Request) => unknown) {
    const previous = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    context.after(() => Reflect.set(globalThis, "window", previous));
    const remote = hostId(remoteHost);
    assert.ok(remote);
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId, remote] }),
        sidebar: async () => assert.fail("预览不得导航或移动会话"),
        async send(value) {
            const packet = value as { hostId: string; request: Request & { id: string } };
            assert.equal(packet.hostId, remote);
            assert.equal(packet.request.params.threadId, identifier);
            assert.match(packet.request.method, /^thread\/(read|turns\/list)$/);
            desktop.receive({
                type: "mcp-response",
                hostId: remote,
                message: { id: packet.request.id, result: reply(packet.request) },
            });
        },
    });
    desktop.knownHosts();
    return () => previewReply(desktop, desktop.capture(remote), identifier);
}

function rawThread(type: Thread["status"]["type"]) {
    return { thread: { id: identifier, status: { type } } };
}

function turn(id: string, status: string, text = "", phase = "final_answer"): Turn {
    return {
        id,
        status,
        items: [{ id: `reply-${id}`, type: "agentMessage", phase, text }],
    };
}

function page(value?: Turn, nextCursor: string | null = null) {
    return { data: value ? [value] : [], nextCursor };
}

test("preview reads the previous complete answer from the verified remote while a turn runs", async (context) => {
    const current = turn("current", "inProgress", "尚未提交");
    const partial = turn("partial", "completed", "只有过程输出", "commentary");
    const previous = turn("previous", "completed", "**最终答复**？");
    const summaryCursors: (string | null | undefined)[] = [];
    const read = fixture(context, ({ method, params }) => {
        if (method === "thread/read") return rawThread("active");
        if (params.itemsView === "notLoaded") return page(current);
        summaryCursors.push(params.cursor);
        if (!params.cursor) return page(current, "partial");
        if (params.cursor === "partial") return page(partial, "previous");
        return page(previous);
    });
    assert.deepEqual(await read(), {
        turnId: "previous",
        html: "<p><strong>最终答复</strong>？</p>\n",
        inProgress: true,
    });
    assert.deepEqual(summaryCursors, [null, "partial", "previous"]);
});

test("preview discards an obsolete response and retries after the current turn changes", async (context) => {
    const obsolete = turn("obsolete", "completed", "旧答复");
    const current = turn("current", "completed", "新答复");
    let starts = 0;
    const read = fixture(context, ({ method, params }) => {
        if (method === "thread/read") return rawThread("idle");
        if (params.itemsView === "notLoaded") return page(current);
        return page(++starts === 1 ? obsolete : current);
    });
    assert.deepEqual(await read(), {
        turnId: "current",
        html: "<p>新答复</p>\n",
        inProgress: false,
    });
    assert.equal(starts, 2);
});

test("preview distinguishes an empty history from malformed pages and runtime read failures", async (context) => {
    let status = "idle";
    let response = page();
    const read = fixture(context, ({ method }) =>
        method === "thread/read" ? rawThread(status) : response,
    );
    assert.deepEqual(await read(), { turnId: null, html: null, inProgress: false });
    response = page(undefined, "not-empty");
    await assert.rejects(read(), /轮次分页无效/);
    response = page(turn("", "completed", "无 ID"));
    await assert.rejects(read(), /轮次 ID 无效/);
    response = page(turn("known", "unrecognized", "未知状态"));
    await assert.rejects(read(), /未知轮次状态/);
    status = "systemError";
    await assert.rejects(read(), /无法核对当前轮次状态/);
});
