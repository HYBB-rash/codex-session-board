import { createApp, shallowRef } from "vue";
import type { BoardSnapshot } from "../controller";
import type { BoardSession } from "../session";
import type { ProjectTag } from "../shared/card/details";
import ArchiveHistory from "./ArchiveHistory.vue";

interface ArchivePorts {
    request(path: string, body?: object): Promise<unknown>;
    snapshot(): BoardSnapshot;
    project(session: BoardSession): ProjectTag;
    date(timestamp?: number): string;
    busy(): boolean;
    movingId(): string | undefined;
    dragActive(): boolean;
    editorOpen(): boolean;
    invalidateSnapshot(): void;
    announce(message: string, error?: boolean): void;
    render(): void;
    load(): Promise<void>;
}

interface ArchiveResult {
    item?: { status: string; message?: string };
}

function message(error: unknown) {
    return error instanceof Error ? error.message : String(error);
}

export function mountBoardArchive(host: HTMLElement, ports: ArchivePorts) {
    const rows = shallowRef<BoardSession[]>([]);
    const submitting = new Set<string>();
    const uncertain = new Map<string, { attemptId: string; message: string }>();
    let readingReceipts = false;
    const app = createApp(ArchiveHistory, {
        sessions: () => rows.value,
        project: (session: BoardSession) => ports.project(session).label,
        date: ports.date,
        read: (id: string) => ports.request("/api/archive-read", { id }),
        restore: restorePlacement,
    });
    app.mount(host);

    function cardActions(session: BoardSession) {
        const readOnly = session.readOnly !== false;
        const busy = ports.busy() || ports.movingId() === session.id;
        const spinning = submitting.has(session.id) || session.archiveChecking === true;
        const archiveMessage = spinning
            ? undefined
            : (session.archiveMessage ?? uncertain.get(session.id)?.message);
        const pending = spinning || session.archivePending === true || uncertain.has(session.id);
        return {
            readOnly,
            spinning,
            archiveMessage,
            canAdjust: !readOnly && !busy && !pending,
            canSubmit:
                !busy && !pending && !session.archiveRestorePending && session.canArchive === true,
        };
    }

    async function submitCard(session: BoardSession) {
        if (!cardActions(session).canSubmit || ports.dragActive() || ports.editorOpen()) return;
        const {
            id,
            version,
            inputVersion,
            sectionVersion,
            updatedAt,
            archiveTurnId,
            archiveInputKey,
        } = session;
        const attemptId = crypto.randomUUID();
        submitting.add(id);
        ports.invalidateSnapshot();
        ports.render();
        try {
            const { item } = (await ports.request("/api/archive", {
                id,
                attemptId,
                version,
                inputVersion,
                sectionVersion,
                updatedAt,
                ...(archiveTurnId !== undefined && { turnId: archiveTurnId }),
                ...(archiveInputKey !== undefined && { inputKey: archiveInputKey }),
            })) as ArchiveResult;
            if (item?.status === "archived" || item?.status === "verified")
                ports.announce("已归档会话");
            else if (item?.status === "rejected" || item?.status === "newInput")
                ports.announce(item.message ?? "宿主拒绝归档", true);
        } catch (error) {
            uncertain.set(id, { attemptId, message: `未收到归档回执：${message(error)}` });
            await reconcileLostReceipts();
        } finally {
            await ports.load();
            submitting.delete(id);
            ports.render();
        }
    }

    async function reconcileLostReceipts() {
        if (readingReceipts || uncertain.size === 0) return;
        readingReceipts = true;
        try {
            const { batches } = (await ports.request("/api/archive-batches")) as {
                batches: { id: string; items: NonNullable<ArchiveResult["item"]>[] }[];
            };
            const receipts = new Map(batches.map(({ id, items }) => [id, items[0]]));
            for (const [id, { attemptId }] of uncertain)
                receiveReceipt(id, receipts.get(attemptId));
        } catch (error) {
            for (const request of uncertain.values())
                request.message = `无法读回归档回执：${message(error)}`;
        } finally {
            readingReceipts = false;
            ports.render();
        }
    }

    function receiveReceipt(id: string, item: ArchiveResult["item"]) {
        if (!item || item.status === "awaiting") return;
        if (item.status === "newInput" || item.status === "rejected")
            ports.announce(item.message ?? "宿主拒绝归档", true);
        uncertain.delete(id);
    }

    async function restorePlacement(id: string) {
        if (submitting.has(id)) return;
        submitting.add(id);
        ports.invalidateSnapshot();
        ports.render();
        try {
            const result = (await ports.request("/api/archive-restore", { id })) as {
                message?: string;
            };
            ports.announce(result.message ?? "会话已恢复", Boolean(result.message));
            return result.message;
        } catch (error) {
            ports.announce(message(error), true);
            return message(error);
        } finally {
            await ports.load();
            submitting.delete(id);
            ports.render();
        }
    }

    function updateBoard(next: BoardSnapshot) {
        rows.value = next.archivedSessions ?? [];
        for (const { id, archivePending, archived } of next.sessions)
            if (archivePending || archived) uncertain.delete(id);
        void reconcileLostReceipts();
    }

    return { cardActions, submitCard, restorePlacement, updateBoard, unmount: () => app.unmount() };
}
