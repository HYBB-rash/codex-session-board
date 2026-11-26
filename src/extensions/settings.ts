import type { BoardMode } from "./placement.js";

export interface BoardSettings {
    mode: BoardMode;
    taskState?: "host" | "migration";
    syncTitles: boolean;
    autoArchiveSeconds: number | null;
    remoteAutoArchiveSeconds: number | null;
    pageSize: number;
    maxPages: number;
    historyPerPoll: number;
    writableThreadIds?: string[];
}

export const defaults: BoardSettings = {
    mode: "four",
    syncTitles: false,
    autoArchiveSeconds: null,
    remoteAutoArchiveSeconds: null,
    pageSize: 100,
    maxPages: 100,
    historyPerPoll: 20,
};

export function readSettings(raw: unknown): BoardSettings {
    if (raw === undefined) return defaults;
    const settings = raw as Partial<BoardSettings> | null;
    if (!settings || typeof settings !== "object") throw new Error("看板配置应为 JSON 对象");
    const value = { ...defaults, ...settings };
    const validInterval = (seconds: number | null) =>
        seconds === null || (Number.isSafeInteger(seconds) && seconds > 0);
    if (
        !["four", "five", "legacy"].includes(value.mode) ||
        (value.taskState !== undefined && !["host", "migration"].includes(value.taskState)) ||
        typeof value.syncTitles !== "boolean" ||
        !validInterval(value.autoArchiveSeconds) ||
        !validInterval(value.remoteAutoArchiveSeconds) ||
        !positiveInteger(value.pageSize) ||
        value.pageSize > 0xffffffff ||
        !positiveInteger(value.maxPages) ||
        !positiveInteger(value.historyPerPoll) ||
        (value.writableThreadIds !== undefined &&
            (!Array.isArray(value.writableThreadIds) ||
                !value.writableThreadIds.every(
                    (id) => typeof id === "string" && /^[\da-f-]{36}$/i.test(id),
                )))
    ) {
        throw new Error("看板状态模式、标题同步、归档周期或分页配置不合法");
    }
    return value;
}

function positiveInteger(value: number): boolean {
    return Number.isSafeInteger(value) && value > 0;
}
