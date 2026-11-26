import type { SidebarContext } from "../../extensions/desktop/connection.js";
import { desktopWorldSource } from "../../extensions/desktop/world.js";
import { findCodexPage, isRunning, readLaunch, readPort } from "./launch.js";

interface Pending {
    resolve(value: CdpReply): void;
    reject(error: Error): void;
    timer: NodeJS.Timeout;
}

export interface CdpReply {
    id?: number;
    method?: string;
    params?: unknown;
    result?: unknown;
    error?: { code?: number; message?: string };
}

export interface CdpConnection {
    call(method: string, params?: object, timeoutMs?: number): Promise<CdpReply>;
    onEvent(listener: (method: string, params: unknown) => void): () => void;
    onClose(listener: () => void): () => void;
    close(): void;
}

function replyError(reply: CdpReply): string | null {
    if (reply.error) return reply.error.message ?? `调试命令失败 (${reply.error.code})`;
    return null;
}

function openSocket(endpoint: URL, timeoutMs: number): Promise<WebSocket> {
    const socket = new WebSocket(endpoint);
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("连接 Codex 调试入口超时")), timeoutMs);
        socket.addEventListener(
            "open",
            () => {
                clearTimeout(timer);
                resolve(socket);
            },
            { once: true },
        );
        socket.addEventListener(
            "error",
            () => {
                clearTimeout(timer);
                reject(new Error("连接 Codex 调试入口失败"));
            },
            { once: true },
        );
    });
}

function settleReply(pending: Map<number, Pending>, reply: CdpReply): void {
    if (typeof reply.id !== "number") return;
    const job = pending.get(reply.id);
    if (!job) return;
    pending.delete(reply.id);
    clearTimeout(job.timer);
    job.resolve(reply);
}

export async function connect(
    webSocketDebuggerUrl: string,
    timeoutMs = 4000,
): Promise<CdpConnection> {
    const endpoint = new URL(webSocketDebuggerUrl);
    if (endpoint.protocol !== "ws:" || endpoint.hostname !== "127.0.0.1")
        throw new Error("Codex 调试入口不是本机回环地址");
    const socket = await openSocket(endpoint, timeoutMs);
    const pending = new Map<number, Pending>();
    const events = new Set<(method: string, params: unknown) => void>();
    const closed = new Set<() => void>();
    let disconnected = false;
    let sequence = 0;
    socket.addEventListener("message", ({ data }) => {
        const reply = JSON.parse(String(data)) as CdpReply;
        if (typeof reply.id === "number") {
            settleReply(pending, reply);
            return;
        }
        if (typeof reply.method !== "string") return;
        for (const listener of events) listener(reply.method, reply.params);
    });
    function close(): void {
        if (disconnected) return;
        disconnected = true;
        socket.close();
        for (const [id, job] of pending) {
            clearTimeout(job.timer);
            job.reject(new Error("Codex 调试入口已断开"));
            pending.delete(id);
        }
        for (const listener of closed) listener();
        closed.clear();
    }
    socket.addEventListener("close", close, { once: true });
    function call(method: string, params: object = {}, callTimeoutMs = 10000): Promise<CdpReply> {
        const id = ++sequence;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                pending.delete(id);
                reject(new Error(`Codex 调试命令超时：${method}`));
            }, callTimeoutMs);
            pending.set(id, { resolve, reject, timer });
            socket.send(JSON.stringify({ id, method, params }));
        });
    }
    function onEvent(listener: (method: string, params: unknown) => void): () => void {
        events.add(listener);
        return () => events.delete(listener);
    }
    function onClose(listener: () => void): () => void {
        closed.add(listener);
        return () => closed.delete(listener);
    }
    return { call, close, onEvent, onClose };
}

export async function evaluate<T>(
    connection: CdpConnection,
    expression: string,
    timeoutMs = 15000,
): Promise<T> {
    const reply = await connection.call(
        "Runtime.evaluate",
        { expression, awaitPromise: true, returnByValue: true },
        timeoutMs,
    );
    const failure = replyError(reply);
    if (failure) throw new Error(failure);
    const outcome = reply.result as
        | {
              result?: { value?: unknown };
              exceptionDetails?: { text?: string; exception?: { description?: string } };
          }
        | undefined;
    if (outcome?.exceptionDetails)
        throw new Error(
            outcome.exceptionDetails.exception?.description ??
                outcome.exceptionDetails.text ??
                "Codex 窗口拒绝执行看板操作",
        );
    return outcome?.result?.value as T;
}

export async function attachOfficial(): Promise<{
    connection: CdpConnection;
    port: number;
    page: { id: string; url: string; webSocketDebuggerUrl: string };
}> {
    const launch = readLaunch();
    if (!launch) throw new Error("官方 Codex 尚未通过看板启动器启动");
    if (!isRunning(launch.pid)) throw new Error("官方 Codex 进程已退出");
    const port = readPort(launch.dataDir);
    if (port === null) throw new Error("官方 Codex 没有可用的调试端口");
    const page = await findCodexPage(port);
    if (!page) throw new Error("官方 Codex 窗口尚未准备好");
    return { connection: await connect(page.webSocketDebuggerUrl), port, page };
}

export async function sidebarContext(connection: CdpConnection): Promise<SidebarContext> {
    const value = await evaluate<unknown>(connection, desktopWorldSource({ kind: "context" }));
    if (
        typeof value !== "object" ||
        value === null ||
        typeof (value as SidebarContext).accountId !== "string" ||
        typeof (value as SidebarContext).userId !== "string" ||
        !Array.isArray((value as SidebarContext).sections)
    )
        throw new Error("官方 Codex 返回的侧栏上下文无效");
    return value as SidebarContext;
}
