import type { BoardMode } from "./placement.js";

export interface BoardSettings {
    mode: BoardMode;
    syncTitles: boolean;
    autoArchiveSeconds: number | null;
    remoteAutoArchiveSeconds: number | null;
    writableThreadIds?: string[];
}

export const defaults: BoardSettings = {
    mode: "four",
    syncTitles: false,
    autoArchiveSeconds: null,
    remoteAutoArchiveSeconds: null,
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
        typeof value.syncTitles !== "boolean" ||
        !validInterval(value.autoArchiveSeconds) ||
        !validInterval(value.remoteAutoArchiveSeconds) ||
        (value.writableThreadIds !== undefined &&
            (!Array.isArray(value.writableThreadIds) ||
                !value.writableThreadIds.every(
                    (id) => typeof id === "string" && /^[\da-f-]{36}$/i.test(id),
                )))
    ) {
        throw new Error("看板状态模式、标题同步或归档周期不合法");
    }
    return value;
}
