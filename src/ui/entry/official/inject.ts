type Listener = (...args: unknown[]) => void;

interface Pending {
    resolve(value: unknown): void;
    reject(error: Error): void;
    timer: number;
}

interface BridgeRequest {
    id: number;
    method: "send" | "invoke";
    channel: string;
    args: unknown[];
}

declare global {
    interface Window {
        __officialBridgeSend?: (value: string) => void;
        __officialBridgeInstalled?: boolean;
        __officialBootstrap?: unknown;
        __officialRequire?: (name: string) => unknown;
        __officialBridgeReply?: (id: number, ok: boolean, value: unknown) => void;
        __officialBridgeEvent?: (channel: string, payload: unknown) => void;
        __officialBridgeRequest?: (
            method: BridgeRequest["method"],
            channel: string,
            args: unknown[],
        ) => Promise<unknown>;
    }
}

const requestTimeoutMs = 60000;
const pending = new Map<number, Pending>();
const listeners = new Map<string, Set<Listener>>();
let sequence = 0;

const scope = window as unknown as {
    process?: { env: Record<string, string | undefined>; platform: string; versions: object };
};
scope.process ??= {
    env: { NODE_ENV: "production" },
    platform: "darwin",
    versions: {},
};
let pagehideRegistered = false;

function emit(channel: string, payload: unknown): void {
    for (const listener of listeners.get(channel) ?? []) listener({}, payload);
}

function request(
    method: BridgeRequest["method"],
    channel: string,
    args: unknown[],
): Promise<unknown> {
    const send = window.__officialBridgeSend;
    if (!send) return Promise.reject(new Error("看板桥接尚未连接"));
    return new Promise((resolve, reject) => {
        const id = ++sequence;
        const timer = window.setTimeout(() => {
            pending.delete(id);
            reject(new Error(`看板请求超时：${channel}`));
        }, requestTimeoutMs);
        pending.set(id, { resolve, reject, timer });
        send(JSON.stringify({ id, method, channel, args } satisfies BridgeRequest));
    });
}

function ipcRenderer() {
    return {
        send(channel: string, ...args: unknown[]): void {
            void request("send", channel, args).catch(() => undefined);
        },
        sendSync(channel: string, ...args: unknown[]): unknown {
            if (channel === "codex_desktop:get-initial-sidebar-bootstrap") {
                if (args.length > 0) return window.__officialBootstrap;
                return window.__officialBootstrap;
            }
            return undefined;
        },
        invoke(channel: string, ...args: unknown[]): Promise<unknown> {
            return request("invoke", channel, args);
        },
        on(channel: string, listener: Listener): void {
            const group = listeners.get(channel) ?? new Set<Listener>();
            group.add(listener);
            listeners.set(channel, group);
        },
        once(channel: string, listener: Listener): void {
            const once: Listener = (...args) => {
                listeners.get(channel)?.delete(once);
                listener(...args);
            };
            const group = listeners.get(channel) ?? new Set<Listener>();
            group.add(once);
            listeners.set(channel, group);
        },
        removeListener(channel: string, listener: Listener): void {
            listeners.get(channel)?.delete(listener);
        },
        removeAllListeners(channel: string): void {
            listeners.delete(channel);
        },
    };
}

function install(): void {
    window.__officialBridgeInstalled = true;
    window.__officialBridgeReply = (id, ok, value) => {
        const job = pending.get(id);
        if (!job) return;
        pending.delete(id);
        window.clearTimeout(job.timer);
        if (ok) job.resolve(value);
        else job.reject(new Error(typeof value === "string" ? value : "看板请求失败"));
    };
    window.__officialBridgeEvent = emit;
    window.__officialBridgeRequest = request;
    window.__officialRequire = (name) => {
        if (name === "electron") return { ipcRenderer: ipcRenderer() };
        throw new Error(`官方窗口不支持模块 ${name}`);
    };
    if (pagehideRegistered) return;
    pagehideRegistered = true;
    window.addEventListener(
        "pagehide",
        () => {
            for (const job of pending.values()) {
                window.clearTimeout(job.timer);
                job.reject(new Error("官方窗口已关闭"));
            }
            pending.clear();
            window.__officialBridgeInstalled = false;
        },
        { once: true },
    );
}

install();
