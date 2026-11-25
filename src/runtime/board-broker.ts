import { randomUUID } from "node:crypto";
import { join } from "node:path";
import type { IpcMain, IpcMainEvent, WebContents } from "electron";
import { HostUnavailableError, startBoardServer } from "../compat/server.js";
import { stateDir } from "../installer/paths.js";
import { boardUpdateStatus } from "../installer/release/store.js";

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

export class BoardBroker {
    private sender: WebContents | undefined;
    private account: string | undefined;
    private readonly pending = new Map<string, Pending>();

    private attach(sender: WebContents, account: unknown): void {
        if (this.sender !== sender) {
            if (this.sender && !this.sender.isDestroyed()) return;
            this.sender = sender;
            sender.on("destroyed", () => {
                if (this.sender !== sender) return;
                this.sender = undefined;
                this.account = undefined;
                for (const [id, job] of this.pending) {
                    if (job.sender === sender) this.settle({ id, error: "Codex 窗口已关闭" });
                }
            });
        }
        this.account = typeof account === "string" ? account : undefined;
    }

    private settle(result: RelayResult): void {
        const job = this.pending.get(result.id);
        if (!job) return;
        clearTimeout(job.timer);
        this.pending.delete(result.id);
        if (result.error !== undefined) job.reject(new Error(result.error));
        else job.resolve(result.value);
    }

    async dispatch(path: string, body?: object): Promise<unknown> {
        if (path === "/api/plugin-update") return boardUpdateStatus(body !== undefined);
        const sender = this.sender;
        const account = this.account;
        if (!sender || sender.isDestroyed() || !account) throw new HostUnavailableError();
        const id = randomUUID();
        const request: BoardRelayRequest = {
            id,
            path,
            account,
            ...(body === undefined ? {} : { body }),
        };
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => this.settle({ id, error: "Codex 看板请求超时" }), 30000);
            this.pending.set(id, { sender, timer, resolve, reject });
            try {
                sender.send("codex-enhancer:board-relay", request);
            } catch (error) {
                this.settle({ id, error: String(error) });
            }
        });
    }

    register(ipcMain: IpcMain): void {
        ipcMain.on("codex-enhancer:board-authority", (event, account) => {
            if (boardFrame(event)) this.attach(event.sender, account);
        });
        ipcMain.on("codex-enhancer:board-result", (event, value) => {
            if (!boardFrame(event) || typeof value !== "object" || value === null) return;
            const result = value as Partial<RelayResult>;
            if (
                typeof result.id === "string" &&
                this.pending.get(result.id)?.sender === event.sender
            ) {
                this.settle(result as RelayResult);
            }
        });
        ipcMain.handle("codex-enhancer:board-update", (event, start) => {
            if (!boardFrame(event)) throw new Error("enhancer IPC rejected");
            return boardUpdateStatus(start === true);
        });
    }

    async listen(log: (event: string, detail?: Record<string, unknown>) => void): Promise<void> {
        const service = await startBoardServer({
            request: this.dispatch.bind(this),
            stateDir: join(stateDir, "session-board"),
            cacheAccount: () => this.account,
            cacheError: (error) => log("board-cache-error", { error: String(error) }),
        });
        log("board-listening", { url: service.url });
    }
}
