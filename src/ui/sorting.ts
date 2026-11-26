import type { BoardSnapshot } from "./controller";
import type { BoardSession } from "./session";

export type Direction = "asc" | "desc";
export interface SortPreference {
    field: "updated_at";
    direction: Direction;
    version: number;
}
export interface SortMenuProps {
    state: string;
    label: string;
    preference: SortPreference;
    disabled: boolean;
    canChange: () => boolean;
    save: (state: string, preference: SortPreference) => Promise<void>;
}

function validTime(value: number) {
    return Number.isFinite(value) && value > 0 && value <= 8640000000000;
}

function compareUpdatedAt(first: BoardSession, second: BoardSession, direction: Direction) {
    const left = validTime(first.updatedAt),
        right = validTime(second.updatedAt);
    if (left !== right) return left ? -1 : 1;
    if (left && first.updatedAt !== second.updatedAt)
        return direction === "asc"
            ? first.updatedAt - second.updatedAt
            : second.updatedAt - first.updatedAt;
    return first.id < second.id ? -1 : first.id > second.id ? 1 : 0;
}

export function sortColumn(
    state: string | null,
    sessions: BoardSession[],
    saved?: Record<string, SortPreference>,
) {
    const preference: SortPreference = (state && saved?.[state]) || {
        field: "updated_at",
        direction: "desc",
        version: 0,
    };
    if (preference.field !== "updated_at") throw new Error("不支持的排序字段，请刷新看板。");
    sessions.sort((first, second) => compareUpdatedAt(first, second, preference.direction));
    return { sessions, preference };
}

interface SortPorts {
    canChange(): boolean;
    snapshot(): BoardSnapshot;
    invalidateSnapshot(): void;
    setSaving(value: boolean): void;
    request(path: "/api/column-sort", body: SortPreference & { state: string }): Promise<unknown>;
    announce(message: string, error?: boolean): void;
    render(resetState: string | null): void;
    load(): Promise<void>;
}

interface SortResult {
    ok?: boolean;
    state?: string;
    preference?: SortPreference;
}

export function createColumnSortSave(ports: SortPorts): SortMenuProps["save"] {
    return async (state, preference) => {
        if (!ports.canChange()) return;
        let changed = false;
        ports.setSaving(true);
        ports.invalidateSnapshot();
        try {
            const result = (await ports.request("/api/column-sort", {
                state,
                ...preference,
            })) as SortResult | null;
            const saved = result?.preference;
            if (
                result?.ok !== true ||
                result.state !== state ||
                !saved ||
                saved.field !== preference.field ||
                saved.direction !== preference.direction ||
                !Number.isSafeInteger(saved.version) ||
                saved.version < preference.version
            )
                throw new Error("排序回执无法确认，请刷新核对。");
            const data = ports.snapshot();
            data.columnSorts = { ...data.columnSorts, [state]: saved };
            changed = true;
            const order = saved.direction === "asc" ? "旧的在前" : "新的在前";
            ports.announce(`${data.states[state]}已按更新时间${order}排列。`);
        } catch (error) {
            ports.announce(error instanceof Error ? error.message : String(error), true);
        } finally {
            ports.setSaving(false);
            ports.render(changed ? state : null);
            void ports.load();
        }
    };
}
