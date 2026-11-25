import { randomUUID } from "node:crypto";
import { join } from "node:path";
import type { IpcMain, IpcMainEvent, WebContents } from "electron";
import { HostUnavailableError, startBoardServer } from "../../compat/server.js";
import { stateDir } from "../../installer/paths.js";
import { boardUpdateStatus } from "../../installer/release/store.js";

export interface BoardRelayRequest {
    id: string;
    path: string;
    account: string;
    body?: object;
}

interface RelayResult {
    id: string;
    value?: unknown;
    error?: string;
}

interface Pending {
    sender: WebContents;
    timer: NodeJS.Timeout;
    resolve(value: unknown): void;
    reject(error: Error): void;
}

export function boardFrame(event: IpcMainEvent): boolean {
    return event.senderFrame?.parent === null && event.senderFrame.url === "app://-/index.html";
}

export function createBoardBroker() {
    let activeSender: WebContents | undefined;
    let activeAccount: string | undefined;
    const pending = new Map<string, Pending>();

    function attach(sender: WebContents, account: unknown): void {
        if (activeSender !== sender) {
            if (activeSender && !activeSender.isDestroyed()) return;
            activeSender = sender;
            sender.on("destroyed", () => {
                if (activeSender !== sender) return;
                activeSender = undefined;
                activeAccount = undefined;
                for (const [id, job] of pending) {
                    if (job.sender === sender) settle({ id, error: "Codex 窗口已关闭" });
                }
            });
        }
        activeAccount = typeof account === "string" ? account : undefined;
    }

    function settle(result: RelayResult): void {
        const job = pending.get(result.id);
        if (!job) return;
        clearTimeout(job.timer);
        pending.delete(result.id);
        if (result.error !== undefined) job.reject(new Error(result.error));
        else job.resolve(result.value);
    }

    async function dispatch(path: string, body?: object): Promise<unknown> {
        if (path === "/api/plugin-update") return boardUpdateStatus(body !== undefined);
        const sender = activeSender;
        const account = activeAccount;
        if (!sender || sender.isDestroyed() || !account) throw new HostUnavailableError();
        const id = randomUUID();
        const request: BoardRelayRequest = {
            id,
            path,
            account,
            ...(body === undefined ? {} : { body }),
        };
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => settle({ id, error: "Codex 看板请求超时" }), 30000);
            pending.set(id, { sender, timer, resolve, reject });
            try {
                sender.send("codex-enhancer:board-relay", request);
            } catch (error) {
                settle({ id, error: String(error) });
            }
        });
    }

    function register(ipcMain: IpcMain): void {
        ipcMain.on("codex-enhancer:board-authority", (event, account) => {
            if (boardFrame(event)) attach(event.sender, account);
        });
        ipcMain.on("codex-enhancer:board-result", (event, value) => {
            if (!boardFrame(event) || typeof value !== "object" || value === null) return;
            const result = value as Partial<RelayResult>;
            if (typeof result.id === "string" && pending.get(result.id)?.sender === event.sender) {
                settle(result as RelayResult);
            }
        });
        ipcMain.handle("codex-enhancer:board-update", (event, start) => {
            if (!boardFrame(event)) throw new Error("enhancer IPC rejected");
            return boardUpdateStatus(start === true);
        });
    }

    async function listen(
        log: (event: string, detail?: Record<string, unknown>) => void,
    ): Promise<void> {
        const service = await startBoardServer({
            request: dispatch,
            stateDir: join(stateDir, "session-board"),
            cacheAccount: () => activeAccount,
            cacheError: (error) => log("board-cache-error", { error: String(error) }),
        });
        log("board-listening", { url: service.url });
    }
    return { register, listen };
}
