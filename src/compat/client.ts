import { join } from "node:path";
import type { BoardRequest } from "./operations.js";
import { object } from "./operations.js";
import { type Receipt, readReceipt } from "./receipt.js";
import { HostUnavailableError } from "./server.js";

export interface BoardClient {
    url: string;
    request: BoardRequest;
    stop(): Promise<unknown>;
}

async function fetchLocal(path: string, receipt: Receipt, body?: object): Promise<unknown> {
    if (!/^\/api\/[a-z/-]+$/.test(path)) throw new Error("本地看板接口无效");
    const headers = { Authorization: `Bearer ${receipt.clientToken}` };
    const url = new URL(path, receipt.url);
    let response: Response;
    try {
        response = await fetch(url, {
            method: body === undefined ? "GET" : "POST",
            headers:
                body === undefined ? headers : { ...headers, "Content-Type": "application/json" },
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
            redirect: "error",
            signal: AbortSignal.timeout(45000),
        });
    } catch (error) {
        if (error instanceof TypeError) throw new HostUnavailableError();
        throw error;
    }
    if (response.headers.get("X-Board-Instance") !== receipt.instance)
        throw new Error("看板服务实例已变更");
    const result: unknown = await response.json();
    if (!response.ok) {
        if (response.status === 503) throw new HostUnavailableError();
        const message = object<"error">(result)?.error;
        throw new Error(typeof message === "string" ? message : "看板操作失败");
    }
    return result;
}

export async function connectBoard(stateDir: string): Promise<BoardClient> {
    const receipt = readReceipt(join(stateDir, "session-board")) ?? readReceipt(stateDir);
    if (!receipt) throw new HostUnavailableError();
    const information = object<"app" | "instance" | "url">(await fetchLocal("/api/info", receipt));
    if (
        information?.app !== receipt.app ||
        information.instance !== receipt.instance ||
        information.url !== receipt.url
    )
        throw new Error("看板服务身份不一致");
    return {
        url: receipt.url,
        request: (path, body) => fetchLocal(path, receipt, body),
        stop: () => fetchLocal("/api/service/stop", receipt, { instance: receipt.instance }),
    };
}
