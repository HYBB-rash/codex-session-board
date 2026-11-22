import type { HostId, LogicalSectionId, SidebarContext, Thread } from "./desktop-connection.js";

export const labels = {
    unknown: "待判断",
    in_progress: "进行中",
    review: "待审阅",
    blocked: "已阻塞",
    to_archive: "待归档",
    completed: "已完成",
} as const;

export type BoardState = keyof typeof labels;
export type BoardMode = "four" | "five" | "legacy";

export function visibleStates(mode: BoardMode): Record<string, string> {
    return Object.fromEntries(
        Object.entries(labels).filter(([state]) => {
            return (
                (mode !== "four" || state !== "unknown") &&
                (mode === "legacy" || state !== "completed")
            );
        }),
    );
}

function matchingSection(context: SidebarContext, state: BoardState) {
    const matches = context.sections.filter(({ name }) => name === labels[state]);
    return matches.length === 1 ? matches[0] : undefined;
}

export function sectionFor(context: SidebarContext, state: BoardState): LogicalSectionId | null {
    return matchingSection(context, state)?.id ?? null;
}

export function sectionBindings(context: SidebarContext, host: HostId, mode: BoardMode) {
    const states = Object.keys(visibleStates(mode)) as BoardState[];
    const candidates = states.map((state) => ({
        state,
        label: labels[state],
        sectionId: matchingSection(context, state)?.hostSectionIds?.[host],
    }));
    const duplicates = new Set(
        candidates
            .map(({ sectionId }) => sectionId)
            .filter((id, index, ids) => id && ids.indexOf(id) !== index),
    );
    return candidates.flatMap(({ state, label, sectionId }) =>
        sectionId && !duplicates.has(sectionId) ? [{ state, label, sectionId }] : [],
    );
}

export interface Placement {
    sectionId: LogicalSectionId | null;
    version: string;
    managed: boolean;
}

export function placement(context: SidebarContext, host: HostId, thread: Thread): Placement {
    const replica = thread.section?.id ?? null;
    const logical =
        replica === null
            ? undefined
            : context.sections.find(({ hostSectionIds }) => hostSectionIds?.[host] === replica);
    return {
        sectionId: logical?.id ?? null,
        version: JSON.stringify([replica, logical?.id ?? null, thread.sectionEnteredAt]),
        managed:
            replica === null ||
            (logical !== undefined &&
                Object.values(labels).some((label) => label === logical.name)),
    };
}

export function presentState(
    context: SidebarContext,
    position: Placement,
    mode: BoardMode,
): BoardState | null {
    const name = context.sections.find(({ id }) => id === position.sectionId)?.name;
    const keys = Object.keys(visibleStates(mode)) as BoardState[];
    return keys.find((key) => labels[key] === name) ?? (mode === "four" ? null : "unknown");
}
