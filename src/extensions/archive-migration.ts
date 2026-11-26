import type { ArchiveMove } from "./archive.js";
import type { HostId, LogicalSectionId, SidebarContext, Thread } from "./desktop/connection.js";
import { oldArchiveSection, placement, uniqueSection } from "./placement.js";

type Attempt = ArchiveMove & { from: LogicalSectionId; to: LogicalSectionId };

export type ArchiveMigrationDecision =
    | { action: "move" | "observed"; attempt: Attempt }
    | { action: "conflict"; reason: string }
    | { action: "unchanged" };

function migrationSection(context: SidebarContext, host: HostId, name: string) {
    const section = uniqueSection(context, name);
    const replica = section?.hostSectionIds?.[host];
    if (
        !replica ||
        context.sections.filter(({ hostSectionIds }) => hostSectionIds?.[host] === replica)
            .length !== 1
    )
        return;
    return { id: section.id, replica };
}

function completedArchiveMove(previous?: ArchiveMove) {
    return Boolean(previous?.after || previous?.observed || previous?.rolledBack);
}

export function archiveMigrationDecision(
    context: SidebarContext,
    host: HostId,
    thread: Thread,
    previous: ArchiveMove | undefined,
    editable: boolean,
): ArchiveMigrationDecision {
    if (completedArchiveMove(previous)) return { action: "unchanged" };
    if (!previous && thread.section?.name !== oldArchiveSection) return { action: "unchanged" };
    const from = migrationSection(context, host, oldArchiveSection);
    const to = migrationSection(context, host, "待审阅");
    if (!from || !to)
        return {
            action: "conflict",
            reason: "旧「待归档」或目标「待审阅」分区缺失、重名或无法核对",
        };
    if (!editable) return { action: "conflict", reason: "该卡片已有其他位置或当前只读" };
    const before = placement(context, host, thread).version;
    const attempt = { from: from.id, to: to.id, before: previous?.before ?? before };
    if (previous && (previous.from !== from.id || previous.to !== to.id))
        return { action: "conflict", reason: "迁移开始后，原生分区已变更" };
    if (previous && thread.section?.id === to.replica) return { action: "observed", attempt };
    if (thread.section?.id !== from.replica || (previous && previous.before !== before))
        return { action: "conflict", reason: "卡片已移到其他位置" };
    return { action: "move", attempt };
}

export function canRollbackArchiveMove(
    context: SidebarContext,
    host: HostId,
    thread: Thread,
    move: ArchiveMove,
) {
    const from = migrationSection(context, host, oldArchiveSection);
    const to = migrationSection(context, host, "待审阅");
    return (
        move.after !== undefined &&
        !move.rolledBack &&
        from?.id === move.from &&
        to?.id === move.to &&
        placement(context, host, thread).version === move.after
    );
}
