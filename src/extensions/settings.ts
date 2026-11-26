import type { BoardMode } from "./placement.js";

export interface BoardSettings {
    mode: BoardMode;
    taskState?: "host" | "migration";
    syncTitles: boolean;
    pageSize: number;
    maxPages: number;
    historyPerPoll: number;
    writableThreadIds?: string[];
}

export const defaults: BoardSettings = {
    mode: "four",
    syncTitles: false,
    pageSize: 100,
    maxPages: 100,
    historyPerPoll: 20,
};

export function readSettings(raw: unknown): BoardSettings {
    if (raw === undefined) return defaults;
    const settings = raw as Partial<BoardSettings> | null;
    if (!settings || typeof settings !== "object") throw new Error("看板配置应为 JSON 对象");
    const { mode, taskState, syncTitles, pageSize, maxPages, historyPerPoll, writableThreadIds } = {
        ...defaults,
        ...settings,
    };
    if (
        !["four", "five", "legacy"].includes(mode) ||
        (taskState !== undefined && !["host", "migration"].includes(taskState)) ||
        typeof syncTitles !== "boolean" ||
        !positiveInteger(pageSize) ||
        pageSize > 0xffffffff ||
        !positiveInteger(maxPages) ||
        !positiveInteger(historyPerPoll) ||
        (writableThreadIds !== undefined &&
            (!Array.isArray(writableThreadIds) ||
                !writableThreadIds.every(
                    (id) => typeof id === "string" && /^[\da-f-]{36}$/i.test(id),
                )))
    ) {
        throw new Error("看板状态模式、标题同步或分页配置不合法");
    }
    return {
        mode,
        syncTitles,
        pageSize,
        maxPages,
        historyPerPoll,
        ...(taskState !== undefined && { taskState }),
        ...(writableThreadIds !== undefined && { writableThreadIds }),
    };
}

function positiveInteger(value: number): boolean {
    return Number.isSafeInteger(value) && value > 0;
}
