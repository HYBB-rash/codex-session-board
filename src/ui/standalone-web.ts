import type { BoardRequest } from "./board-start";
import { startBoard } from "./standalone-page";

async function requestToken(): Promise<string> {
    const response = await fetch("/api/info", { cache: "no-store", redirect: "error" });
    if (!response.ok) throw new Error("本地看板服务未连接");
    const result = await response.json();
    if (typeof result.requestToken !== "string") throw new Error("无法读取本地看板请求令牌");
    return result.requestToken;
}

const transport: BoardRequest = async (path, body) => {
    const response = await fetch(path, body === undefined ? { cache: "no-store", redirect: "error" } : {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-CSRF-Token": await requestToken() },
        body: JSON.stringify(body),
        cache: "no-store",
        redirect: "error",
    });
    const value = await response.json();
    if (!response.ok) throw new Error(value.error || "本地看板操作失败");
    return value;
};

startBoard(transport);
void import("./board-pwa");
