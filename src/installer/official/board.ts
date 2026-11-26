import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import * as path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { type DesktopCommand, desktopWorldSource } from "../../extensions/desktop/world.js";
import { createBoardStorage } from "../../runtime/board/storage.js";
import { logs, outputDir, stateDir } from "../paths.js";
import { boardUpdateStatus } from "../release/store.js";
import { attachOfficial, type CdpConnection, evaluate } from "./attach.js";
import { isRunning, readLaunch } from "./launch.js";

export interface BoardService {
    pid: number;
    appPid: number;
    port: number;
    startedAt: string;
}

export interface BoardStatus {
    service: BoardService | null;
    running: boolean;
    injected: boolean;
}

interface BridgeRequest {
    id: number;
    method: "send" | "invoke";
    channel: string;
    args: unknown[];
}

const item = {
    id: "session-board",
    title: "会话看板",
    route: "/session-board",
    icon: "layout-dashboard",
    page: {
        title: "会话看板",
        body: "当前窗口无法挂载会话看板。",
    },
};

const receiptPath = path.join(stateDir, "official", "board.json");
const logPath = path.join(logs, "official-board.log");

function readBoard(): BoardService | null {
    try {
        const value = JSON.parse(readFileSync(receiptPath, "utf8")) as Partial<BoardService>;
        if (
            typeof value.pid !== "number" ||
            typeof value.appPid !== "number" ||
            typeof value.port !== "number" ||
            typeof value.startedAt !== "string"
        )
            return null;
        return value as BoardService;
    } catch {
        return null;
    }
}

function writeBoard(service: BoardService): void {
    mkdirSync(path.dirname(receiptPath), { recursive: true, mode: 0o700 });
    writeFileSync(receiptPath, `${JSON.stringify(service, null, 2)}\n`, { mode: 0o600 });
}

function clearBoard(): void {
    rmSync(receiptPath, { force: true });
}

export function logBoard(event: string, detail: Record<string, unknown> = {}): void {
    mkdirSync(path.dirname(logPath), { recursive: true, mode: 0o700 });
    writeFileSync(
        logPath,
        `${JSON.stringify({ at: new Date().toISOString(), event, ...detail })}\n`,
        { mode: 0o600, flag: "a" },
    );
}

function bridgeReply(connection: CdpConnection, id: number, ok: boolean, value: unknown): void {
    const payload = JSON.stringify(value ?? null);
    void evaluate(
        connection,
        `window.__officialBridgeReply?.(${id}, ${ok}, ${payload})`,
        5000,
    ).catch((error: unknown) => logBoard("bridge-reply-failed", { error: String(error) }));
}

async function invoke(
    connection: CdpConnection,
    storage: ReturnType<typeof createBoardStorage>,
    channel: string,
    args: unknown[],
): Promise<unknown> {
    switch (channel) {
        case "codex-enhancer:ui-items":
            return { items: [item], autoOpen: false };
        case "codex-enhancer:task-cache":
            return storage.taskCache(args[0] as string, args.length > 1 ? args[1] : undefined);
        case "codex-enhancer:board-ledger":
            return storage.ledger(args[0] as string, args.length > 1 ? args[1] : undefined);
        case "codex-enhancer:board-config":
            return storage.config(args[0] as string);
        case "codex-enhancer:board-title-epoch":
            return storage.titleEpoch(args[0] as string);
        case "codex-enhancer:board-phase-intent":
            return storage.phaseIntent(args[0]);
        case "codex-enhancer:board-records":
            return storage.record(args[0]);
        case "codex-enhancer:board-title-result":
            return storage.titleResult(args[0]);
        case "codex-enhancer:board-update":
            return boardUpdateStatus(args[0] === true);
        case "codex-enhancer:board-desktop":
            return evaluate(connection, desktopWorldSource(args[0] as DesktopCommand), 30000);
        case "codex_desktop:message-from-view":
            return evaluate(
                connection,
                `window.electronBridge.sendMessageFromView(${JSON.stringify(args[0])})`,
                30000,
            );
        case "codex_desktop:get-initial-sidebar-bootstrap":
            return evaluate(
                connection,
                `window.electronBridge.getInitialSidebarBootstrap()`,
                15000,
            );
        default:
            throw new Error(`未知的看板通道：${channel}`);
    }
}

function receive(
    connection: CdpConnection,
    storage: ReturnType<typeof createBoardStorage>,
    binding: string,
    value: unknown,
): void {
    if (typeof value !== "object" || value === null) return;
    const params = value as { name?: unknown; payload?: unknown };
    if (params.name !== binding || typeof params.payload !== "string") return;
    let request: BridgeRequest;
    try {
        request = JSON.parse(params.payload) as BridgeRequest;
    } catch {
        return;
    }
    if (process.env.CODEX_ENHANCER_DEBUG_BRIDGE === "1")
        logBoard("bridge-call", {
            method: request.method,
            channel: request.channel,
            id: request.id,
        });
    if (request.method === "send") {
        if (request.channel === "codex-enhancer:proof")
            logBoard("proof", { value: request.args[0] });
        bridgeReply(connection, request.id, true, null);
        return;
    }
    void invoke(connection, storage, request.channel, request.args).then(
        (result) => bridgeReply(connection, request.id, true, result),
        (error: unknown) =>
            bridgeReply(
                connection,
                request.id,
                false,
                error instanceof Error ? error.message : String(error),
            ),
    );
}

async function inject(
    connection: CdpConnection,
    storage: ReturnType<typeof createBoardStorage>,
): Promise<void> {
    await connection.call("Runtime.enable");
    await connection.call("Page.enable");
    const binding = `__officialBridgeSend_${randomUUID().replaceAll("-", "")}`;
    const added = await connection.call("Runtime.addBinding", { name: binding });
    if (added.error) throw new Error(added.error.message ?? "无法建立看板桥接");
    connection.onEvent((method, params) => {
        if (method === "Runtime.bindingCalled") receive(connection, storage, binding, params);
    });
    const shim = readFileSync(path.join(outputDir, "official", "inject.js"), "utf8");
    await evaluate(connection, shim, 10000);
    await evaluate(
        connection,
        `window.__officialBridgeSend = window[${JSON.stringify(binding)}]; true`,
        5000,
    );
    const installed = await evaluate<boolean>(
        connection,
        "window.__officialBridgeInstalled === true",
    );
    if (!installed) throw new Error("官方窗口没有接受看板桥接");
    await evaluate(
        connection,
        `(async () => {
            window.__officialBootstrap = await window.electronBridge.getInitialSidebarBootstrap();
            return true;
        })()`,
        20000,
    );
    const preload = readFileSync(path.join(outputDir, "runtime", "preload.js"), "utf8");
    await evaluate(
        connection,
        `(() => {
            const module = { exports: {} };
            (function (require, module, exports) {\n${preload}\n})(window.__officialRequire, module, module.exports);
            return true;
        })()`,
        30000,
    );
    const deadline = Date.now() + 10000;
    while (Date.now() < deadline) {
        const mounted = await evaluate<boolean>(
            connection,
            `Boolean(document.querySelector('[data-codex-enhancer-item="session-board"]'))`,
            5000,
        );
        if (mounted) return;
        await delay(250);
    }
    throw new Error("会话看板入口没有出现在原生侧栏");
}

function waitForDisconnect(connection: CdpConnection, appPid: number): Promise<void> {
    return new Promise((resolve) => {
        let done = false;
        const finish = (): void => {
            if (done) return;
            done = true;
            clearInterval(timer);
            offClose();
            offEvent();
            resolve();
        };
        const offClose = connection.onClose(finish);
        const offEvent = connection.onEvent((method) => {
            if (method === "Inspector.targetCrashed" || method === "Page.loadEventFired") finish();
        });
        const timer = setInterval(() => {
            if (!isRunning(appPid)) finish();
        }, 2000);
    });
}

async function attachOnce(appPid: number, storage: ReturnType<typeof createBoardStorage>) {
    const attached = await attachOfficial();
    try {
        await inject(attached.connection, storage);
        writeBoard({
            pid: process.pid,
            appPid,
            port: attached.port,
            startedAt: new Date().toISOString(),
        });
        logBoard("board-injected", { pid: process.pid, appPid, port: attached.port });
        await waitForDisconnect(attached.connection, appPid);
    } finally {
        attached.connection.close();
        if (readBoard()?.pid === process.pid) clearBoard();
    }
}

async function runOfficialBoard(): Promise<void> {
    const launch = readLaunch();
    if (!launch) throw new Error("请先通过看板启动器启动官方 Codex");
    const storage = createBoardStorage(path.join(stateDir, "session-board"));
    let stopping = false;
    process.once("SIGTERM", () => {
        stopping = true;
    });
    process.once("SIGINT", () => {
        stopping = true;
    });
    let failures = 0;
    while (!stopping) {
        try {
            await attachOnce(launch.pid, storage);
            failures = 0;
        } catch (error) {
            failures++;
            logBoard("board-attach-failed", {
                error: error instanceof Error ? error.message : String(error),
                failures,
            });
            if (failures >= 5) throw error;
            await delay(2000);
        }
        if (!stopping && isRunning(launch.pid)) await delay(1000);
    }
    logBoard("board-stopped", { pid: process.pid });
}

export async function serveOfficialBoard(): Promise<void> {
    try {
        await runOfficialBoard();
    } catch (error) {
        logBoard("serve-failed", {
            error: error instanceof Error ? error.message : String(error),
            stack: error instanceof Error ? (error.stack ?? null) : null,
        });
        throw error;
    }
}

export async function startOfficialBoard(appPid: number): Promise<BoardService> {
    const active = readBoard();
    if (active && active.appPid === appPid && isRunning(active.pid)) return active;
    mkdirSync(logs, { recursive: true, mode: 0o700 });
    const child = spawn(
        process.execPath,
        [path.join(outputDir, "bin", "codex-enhancer.js"), "official", "serve"],
        {
            cwd: path.dirname(outputDir),
            env: process.env,
            detached: true,
            stdio: "ignore",
        },
    );
    child.unref();
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
        const service = readBoard();
        if (service && isRunning(service.pid)) return service;
        await delay(250);
    }
    throw new Error(`看板服务没有启动，请查看 ${logPath}`);
}

export async function boardStatus(): Promise<BoardStatus> {
    const service = readBoard();
    if (!service) return { service: null, running: false, injected: false };
    const running = isRunning(service.pid);
    let injected = false;
    if (running) {
        try {
            const { connection } = await attachOfficial();
            try {
                injected = await evaluate<boolean>(
                    connection,
                    `Boolean(window.__codexEnhancerHost)`,
                    4000,
                );
            } finally {
                connection.close();
            }
        } catch {
            injected = false;
        }
    }
    return { service, running, injected };
}

export async function stopOfficialBoard(): Promise<boolean> {
    const service = readBoard();
    if (!service) return false;
    if (isRunning(service.pid)) {
        process.kill(service.pid, "SIGTERM");
        const deadline = Date.now() + 5000;
        while (Date.now() < deadline && isRunning(service.pid)) await delay(200);
        if (isRunning(service.pid)) process.kill(service.pid, "SIGKILL");
    }
    clearBoard();
    return true;
}
