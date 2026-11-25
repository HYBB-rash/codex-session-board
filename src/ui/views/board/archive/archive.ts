import type { BoardSession } from "../../../board-view";
import type { BoardSnapshot } from "../controller";
import type { ArchiveRequests, ArchiveTarget } from "./batches";
import { mountArchiveResults } from "./results";

export interface AutoArchiveHealth {
    autoArchiveSeconds?: number;
    autoArchiveNextAt?: number;
    autoArchiveLastRun?: {
        at: number;
        archived: string[];
        deferred: { id: string; reason: string }[];
    } | null;
    autoArchiveError?: string | null;
}

interface ArchivePorts {
    request: ArchiveRequests["request"];
    snapshot(): BoardSnapshot;
    open(id: string): void;
    date(timestamp?: number): string;
    persist(session: BoardSession, state: "to_archive", note: string): Promise<unknown>;
    busy(): boolean;
    movingId(): string | undefined;
    dragActive(): boolean;
    editorOpen(): boolean;
    setSaving(value: boolean): void;
    invalidateSnapshot(): void;
    announce(message: string, error?: boolean): void;
    render(): void;
    load(): Promise<void>;
}

function mountResults(host: HTMLElement, ports: ArchivePorts) {
    const { snapshot, render, load } = ports;
    return mountArchiveResults(host, {
        request: ports.request,
        open: ports.open,
        date: ports.date,
        changed: render,
        refresh(ids) {
            ports.invalidateSnapshot();
            for (const card of snapshot().sessions)
                if (ids.has(card.id)) card.archivePending = true;
            render();
            void load();
        },
    });
}

function actionsForCard(
    session: BoardSession,
    ports: ArchivePorts,
    results: ReturnType<typeof mountResults>,
) {
    const readOnly = session.readOnly !== false;
    const canAdjust = !readOnly && !ports.busy() && ports.movingId() !== session.id;
    const pendingArchive = session.archivePending === true;
    const archive =
        session.state === "to_archive" && session.canArchive === true && !pendingArchive
            ? session.followUp
                ? "confirm"
                : "archive"
            : null;
    const canSubmit = canAdjust && archive !== null && results.canSubmit;
    return {
        readOnly,
        canAdjust,
        pendingArchive,
        archive,
        canSubmit,
        inBatch: canSubmit && archive === "archive",
    };
}

async function confirmInputs(
    session: BoardSession,
    actions: ReturnType<typeof actionsForCard>,
    ports: ArchivePorts,
) {
    const { render, load, announce } = ports;
    if (
        !actions.canSubmit ||
        actions.archive !== "confirm" ||
        ports.dragActive() ||
        ports.editorOpen()
    )
        return;
    ports.setSaving(true);
    render();
    try {
        await ports.persist(session, "to_archive", session.note);
        announce("已确认待归档。现在可以点击归档或一键归档。");
    } catch (error) {
        announce(error instanceof Error ? error.message : String(error), true);
    } finally {
        ports.setSaving(false);
        render();
        await load();
    }
}

async function submitArchive(
    sessions: BoardSession[],
    eligible: (session: BoardSession) => boolean,
    results: ReturnType<typeof mountResults>,
    ports: ArchivePorts,
) {
    const { render, load, announce } = ports;
    if (ports.busy() || ports.dragActive() || !results.canSubmit || ports.editorOpen()) return;
    const items = sessions.filter(eligible).map(
        ({ id, title, version, inputVersion, sectionVersion }): ArchiveTarget => ({
            id,
            title,
            version,
            inputVersion,
            sectionVersion: sectionVersion ?? null,
        }),
    );
    if (!items.length) {
        announce("当前列没有可归档的会话。");
        return;
    }
    ports.setSaving(true);
    ports.invalidateSnapshot();
    render();
    try {
        await results.submit(items);
    } finally {
        ports.setSaving(false);
        render();
        await load();
    }
}

function updateStatus(status: HTMLElement, ports: ArchivePorts) {
    const { autoArchiveSeconds, autoArchiveNextAt, autoArchiveLastRun, autoArchiveError } =
        ports.snapshot().health;
    status.hidden = !autoArchiveSeconds;
    if (!autoArchiveSeconds) return;
    const deferred = autoArchiveLastRun?.deferred ?? [];
    status.className = `health${autoArchiveError || deferred.length ? " error" : ""}`;
    let text = `本机每 ${autoArchiveSeconds / 60} 分钟自动归档待归档 · 下次 ${ports.date(autoArchiveNextAt)}`;
    if (autoArchiveLastRun) {
        const { at, archived } = autoArchiveLastRun;
        text += ` · 上次 ${ports.date(at)}：归档 ${archived.length} 条，延后 ${deferred.length} 条`;
    }
    text += " · 不受当前筛选影响";
    if (autoArchiveError) text += " · 本轮未完成，请查看连接详情";
    status.textContent = text;
    status.title = deferred.map((item) => `${item.id}：${item.reason}`).join("\n");
}

export function mountBoardArchive(host: HTMLElement, status: HTMLElement, ports: ArchivePorts) {
    const results = mountResults(host, ports);
    const cardActions = (session: BoardSession) => actionsForCard(session, ports, results);
    const eligible = (session: BoardSession) => cardActions(session).inBatch;
    const confirm = (session: BoardSession) => confirmInputs(session, cardActions(session), ports);
    const submit = (sessions: BoardSession[]) => submitArchive(sessions, eligible, results, ports);
    return {
        cardActions,
        confirmInputs: confirm,
        submit,
        updateStatus: () => updateStatus(status, ports),
        unmount: results.unmount,
        submitCard: (session: BoardSession) =>
            cardActions(session).archive === "confirm" ? confirm(session) : submit([session]),
        count: (sessions: BoardSession[]) => sessions.filter(eligible).length,
        get pending() {
            return results.pending;
        },
        updateBoard: results.updateBoard,
        restore: () => (ports.snapshot().legacyMode === true ? undefined : results.restore()),
    };
}
