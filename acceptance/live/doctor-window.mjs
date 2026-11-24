import { setTimeout as delay } from "node:timers/promises";

const [port, target, fixture] = process.argv.slice(2);
const address = `127.0.0.1:${port}`;

function identifyWindow() {
    return {
        sidebar: Boolean(document.querySelector("[data-app-action-sidebar-scroll]")),
        enhancer: Boolean(document.querySelector('[data-codex-enhancer-item="session-board"]')),
        nativeBridge: typeof window.electronBridge?.sendMessageFromView === "function",
        focused: document.hasFocus(),
        visible: document.visibilityState === "visible",
    };
}

function readNative({ hostId, method, threadId }) {
    const id = `board-accept-doctor-${crypto.randomUUID()}`;
    const params =
        method === "account/read"
            ? { refreshToken: false }
            : method === "thread/list"
              ? { limit: 1, archived: false, useStateDbOnly: true }
              : { threadId, includeTurns: false };
    return new Promise((resolve) => {
        const finish = (result) => {
            clearTimeout(timer);
            window.removeEventListener("message", receive);
            resolve(result);
        };
        const receive = ({ data }) => {
            if (data?.type !== "mcp-response" || data.hostId !== hostId || data.message?.id !== id)
                return;
            if (data.message.error) return finish({ error: data.message.error.code });
            const result = data.message.result;
            if (method === "account/read") return finish({ type: result?.account?.type ?? null });
            if (method === "thread/list") return finish({ connected: Array.isArray(result?.data) });
            return finish({ readable: result?.thread?.id === threadId });
        };
        const timer = setTimeout(() => finish({ timedOut: true }), 8000);
        window.addEventListener("message", receive);
        window.electronBridge.sendMessageFromView({
            type: "mcp-request",
            hostId,
            request: { id, method, params },
        });
    });
}

async function attach(page) {
    const endpoint = new URL(page.webSocketDebuggerUrl);
    if (endpoint.protocol !== "ws:" || endpoint.host !== address) throw Error("调试地址不符");
    const socket = new WebSocket(endpoint);
    try {
        await Promise.race([
            new Promise((resolve, reject) => {
                socket.addEventListener("open", resolve, { once: true });
                socket.addEventListener("error", reject, { once: true });
            }),
            delay(3000, undefined, { ref: false }).then(() => {
                throw Error("连接调试窗口超时");
            }),
        ]);
    } catch (error) {
        socket.close();
        throw error;
    }
    let sequence = 0;
    const evaluate = (read, args) =>
        new Promise((resolve, reject) => {
            const id = ++sequence;
            const finish = (value, error) => {
                clearTimeout(timer);
                socket.removeEventListener("message", onMessage);
                if (error) reject(error);
                else resolve(value);
            };
            const onMessage = ({ data }) => {
                const reply = JSON.parse(data);
                if (reply.id !== id) return;
                if (reply.error || reply.result?.exceptionDetails)
                    return finish(null, Error("原生窗口拒绝只读探针"));
                finish(reply.result.result.value);
            };
            const timer = setTimeout(() => finish(null, Error("读取原生窗口超时")), 11000);
            socket.addEventListener("message", onMessage);
            socket.send(
                JSON.stringify({
                    id,
                    method: "Runtime.evaluate",
                    params: {
                        expression: `(${read})(${JSON.stringify(args)})`,
                        returnByValue: true,
                        awaitPromise: true,
                    },
                }),
            );
        });
    return { evaluate, close: () => socket.close() };
}

const clients = [];
try {
    const response = await fetch(`http://${address}/json/list`, {
        signal: AbortSignal.timeout(3000),
    });
    if (!response.ok) throw Error("无法枚举调试窗口");
    const pages = (await response.json()).filter(
        ({ type, url }) => type === "page" && /^app:\/\/-\/index\.html(?:[?#]|$)/.test(url),
    );
    if (!pages.length || pages.length > 5) throw Error("没有可确认的 Codex 窗口");
    for (const page of pages) clients.push(await attach(page));
    const available = await Promise.all(
        clients.map(async (client) => ({ client, ...(await client.evaluate(identifyWindow)) })),
    );
    const usable = available.filter(
        ({ sidebar, enhancer, nativeBridge }) => sidebar && enhancer && nativeBridge,
    );
    const selected =
        usable.find(({ focused }) => focused) ?? usable.find(({ visible }) => visible) ?? usable[0];
    if (!selected) throw Error("找不到属于 Enhancer 的原生 Codex 窗口");
    const { client } = selected;
    const checks = [client.evaluate(readNative, { hostId: "local", method: "account/read" })];
    if (target) checks.push(client.evaluate(readNative, { hostId: target, method: "thread/list" }));
    if (fixture)
        checks.push(
            client.evaluate(readNative, {
                hostId: target,
                method: "thread/read",
                threadId: fixture,
            }),
        );
    const [account, connection, card] = await Promise.all(checks);
    console.log(JSON.stringify({ account, connection, card }));
} catch (error) {
    console.log(JSON.stringify({ error: error.message || "无法连接原生窗口" }));
    process.exitCode = 1;
} finally {
    for (const client of clients) client.close();
}
