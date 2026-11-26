import type { HostId, LogicalSectionId, SidebarContext, Thread } from "./desktop/connection.js";
import type { CardRecord } from "./records.js";

export const labels = {
    unknown: "待判断",
    in_progress: "进行中",
    review: "待审阅",
    deferred: "待处理",
    blocked: "已阻塞",
    completed: "已完成",
} as const;

export type BoardState = keyof typeof labels;
export type BoardMode = "four" | "five" | "legacy";
export const oldArchiveSection = "待归档";

export function migratedState<T>(state: T): Exclude<T, "to_archive"> | "review" {
    return state === "to_archive" ? "review" : (state as Exclude<T, "to_archive">);
}

export function migrateArchiveRecord(record: CardRecord): CardRecord {
    return {
        ...record,
        ...(record.state !== undefined && { state: migratedState(record.state) }),
        ...(record.title && {
            title: { ...record.title, state: migratedState(record.title.state) },
        }),
    };
}

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
    return uniqueSection(context, labels[state]);
}

export function uniqueSection(context: SidebarContext, name: string) {
    const matches = context.sections.filter((section) => section.name === name);
    return matches.length === 1 ? matches[0] : undefined;
}

export function sectionFor(
    context: SidebarContext,
    state: BoardState,
    host?: HostId,
): LogicalSectionId | null {
    const section = matchingSection(context, state);
    if (!section) return null;
    if (host !== undefined) {
        const replica = section.hostSectionIds?.[host];
        if (
            replica &&
            context.sections.some(
                (other) => other !== section && other.hostSectionIds?.[host] === replica,
            )
        )
            return null;
    }
    return section.id;
}

export function sectionBindings(context: SidebarContext, host: HostId, mode: BoardMode) {
    const states = Object.keys(visibleStates(mode)) as BoardState[];
    const candidates = states.map((state) => ({
        state,
        label: labels[state],
        sectionId:
            state === "deferred" && !sectionFor(context, state, host)
                ? undefined
                : matchingSection(context, state)?.hostSectionIds?.[host],
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
    const matches =
        replica === null
            ? []
            : context.sections.filter(({ hostSectionIds }) => hostSectionIds?.[host] === replica);
    const logical = matches.find(({ name }) => name !== labels.deferred) ?? matches[0];
    return {
        sectionId: logical?.id ?? null,
        version: JSON.stringify([replica, logical?.id ?? null, thread.sectionEnteredAt]),
        managed:
            replica === null ||
            (logical !== undefined &&
                (logical.name === oldArchiveSection ||
                    Object.values(labels).some((label) => label === logical.name))),
    };
}

export function presentState(
    context: SidebarContext,
    position: Placement,
    mode: BoardMode,
): BoardState | null {
    const name = context.sections.find(({ id }) => id === position.sectionId)?.name;
    if (name === oldArchiveSection) return "review";
    const keys = Object.keys(visibleStates(mode)) as BoardState[];
    return keys.find((key) => labels[key] === name) ?? (mode === "four" ? null : "unknown");
}
