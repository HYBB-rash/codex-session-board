import assert from "node:assert/strict";
import { readFileSync, readlinkSync } from "node:fs";
import { setTimeout as delay } from "node:timers/promises";

const log = readFileSync(new URL("../logs/desktop.log", import.meta.url), "utf8");
const proof = readFileSync(new URL("../logs/runtime.jsonl", import.meta.url), "utf8");
const started = proof
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line))
    .findLast(({ event }) => event === "loader-entered");
assert.equal(
    readlinkSync(`/proc/${started.pid}/exe`),
    new URL("../state/test-app/ChatGPT", import.meta.url).pathname,
);
const port = [...log.matchAll(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)/g)].at(-1)?.[1];
assert.ok(port);
const pages = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());
const target = pages.find(({ url, type }) => type === "page" && url === "app://-/index.html");
assert.ok(target);
const threadId = "01a2030f-7145-7ba0-9cb8-0c13b91df59e";
const hostId = "remote-ssh-discovered:macbook";
const base = "http://127.0.0.1:18767";
const marker = `BOARD-OBSERVE-${crypto.randomUUID().slice(0, 8)}`;

function connect(url) {
    const socket = new WebSocket(url);
    const ready = new Promise((resolve, reject) => {
        socket.onopen = resolve;
        socket.onerror = reject;
    });
    let next = 0;
    const calls = new Map();
    socket.onmessage = ({ data }) => {
        const frame = JSON.parse(data);
        calls.get(frame.id)?.(frame);
        calls.delete(frame.id);
    };
    return {
        async request(method, params) {
            await ready;
            const id = ++next;
            const result = await new Promise((resolve) => {
                calls.set(id, resolve);
                socket.send(JSON.stringify({ id, method, params }));
            });
            const error = result.error ?? result.result?.exceptionDetails;
            if (error) throw new Error(`${method} ${error.text ?? error.message}`);
        },
        close: () => socket.close(),
    };
}

async function currentCard() {
    const response = await fetch(`${base}/api/sessions`);
    assert.ok(response.ok);
    const snapshot = await response.json();
    const card = snapshot.sessions.find(
        (row) => row.hostId === hostId && row.threadId === threadId,
    );
    assert.ok(card && !card.archived && !card.readOnly, "专用远程会话必须处于可写状态");
    return card;
}

const guard = `
    const row = [...document.querySelectorAll('[data-app-action-sidebar-thread-row]')].find(e =>
        e.getAttribute('data-app-action-sidebar-thread-id') === 'local:${threadId}' &&
        e.getAttribute('data-app-action-sidebar-thread-host-id') === '${hostId}');
    if (!row) throw new Error('Missing fixture');
`;

async function sendMarker(client) {
    const evaluate = (source) =>
        client.request("Runtime.evaluate", { expression: `(()=>{${guard}${source}})()` });
    await evaluate("row.click()");
    await delay(700);
    await evaluate(`
        if (!row.matches('[aria-current],[data-active=true]')) throw new Error('Wrong conversation');
        const visible = [...document.querySelectorAll('.ProseMirror[contenteditable=true]')]
            .filter(e => e.clientWidth);
        if (visible.length !== 1 || visible[0].textContent.trim()) throw new Error('Wrong editor');
        visible[0].focus();
    `);
    await client.request("Input.insertText", {
        text: `在本轮验收中等待约 8 秒，再只回复 ${marker}。`,
    });
    await evaluate(`
        if (!row.matches('[aria-current],[data-active=true]') ||
            !document.activeElement?.matches('.ProseMirror[contenteditable=true]') ||
            !document.activeElement.textContent.includes('${marker}')) throw new Error('Wrong target');
    `);
    const enter = {
        key: "Enter",
        code: "Enter",
        windowsVirtualKeyCode: 13,
        nativeVirtualKeyCode: 13,
    };
    await client.request("Input.dispatchKeyEvent", { ...enter, type: "keyDown" });
    await client.request("Input.dispatchKeyEvent", { ...enter, type: "keyUp" });
}

async function observe(before) {
    const observed = new Set();
    for (let attempt = 0; attempt < 40; attempt++) {
        const card = await currentCard();
        assert.equal(card.note, before.note);
        observed.add(card.state);
        const finished = card.state === "review" && card.previewTurnId !== before.previewTurnId;
        if (finished && card.title.startsWith("[待审阅]") && !card.titleSyncPending) {
            assert.equal(card.inputVersion, before.inputVersion + 1);
            assert.ok(observed.has("in_progress"), "未观察到真实进行中状态");
            console.log(
                JSON.stringify({
                    hostId,
                    threadId,
                    observed: [...observed],
                    inputVersion: card.inputVersion,
                    turnId: card.previewTurnId,
                    noteKept: true,
                    titleReadBack: true,
                }),
            );
            return;
        }
        await delay(1500);
    }
    throw new Error("未读回新的完整轮次");
}

if (!process.argv.includes("--send-fixture")) throw new Error("验收会向专用会话发送一条提示");
const before = await currentCard();
const client = connect(target.webSocketDebuggerUrl);
try {
    await sendMarker(client);
    await observe(before);
} finally {
    client.close();
}
