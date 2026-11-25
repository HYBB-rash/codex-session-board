import type { BoardSnapshot } from "../../../board-start";
import type { BoardSession } from "../../../board-view";
import { mountCardEditor } from "./mount-editor";
import type { mountMoveFeedback } from "./move-feedback";

interface EditsPorts {
    snapshot(): BoardSnapshot;
    invalidateSnapshot(): void;
    request(path: "/api/sessions" | "/api/state", body?: object): Promise<unknown>;
    movePanel(): ReturnType<typeof mountMoveFeedback>;
    setSaving(value: boolean): void;
    busy(): boolean;
    canAdjust(session: BoardSession): boolean;
    announce(message: string, error?: boolean): void;
    render(): void;
    load(): Promise<void>;
}

interface StateResult {
    ok?: boolean;
    session?: BoardSession;
    titleEnabled?: boolean;
    titlePending?: boolean;
    titleError?: string | null;
}

function sameCard(row: BoardSession | undefined, session: BoardSession) {
    return (
        row?.id === session.id &&
        row.accountScope === session.accountScope &&
        (row.hostId || "local") === (session.hostId || "local") &&
        (row.threadId || row.id) === (session.threadId || session.id)
    );
}

async function persist(
    ports: EditsPorts,
    session: BoardSession,
    state: string | null,
    note?: string,
) {
    ports.invalidateSnapshot();
    const { version, inputVersion, sectionVersion } = session;
    const result = (await ports.request("/api/state", {
        id: session.id,
        state,
        note,
        version,
        inputVersion,
        sectionVersion,
    })) as StateResult | null;
    const confirmed = result?.session;
    const data = ports.snapshot();
    if (
        result?.ok !== true ||
        !confirmed ||
        !sameCard(confirmed, session) ||
        typeof confirmed.note !== "string" ||
        !Number.isSafeInteger(confirmed.version) ||
        !Number.isSafeInteger(confirmed.inputVersion) ||
        !(confirmed.state === null || Object.hasOwn(data.states, confirmed.state))
    )
        throw new Error("保存回执缺少有效卡片，请刷新核对。");
    const current = data.sessions.find((row) => sameCard(row, session));
    if (
        confirmed.version <= version ||
        confirmed.version < (current?.version ?? 0) ||
        confirmed.inputVersion < Math.max(inputVersion, current?.inputVersion ?? 0)
    )
        throw new Error("收到旧保存回执，请重新核对当前卡片。");
    data.sessions = data.sessions.map((row) => (sameCard(row, session) ? confirmed : row));
    return { ...result, session: confirmed };
}

function reconcile(unconfirmed: Map<string, BoardSession>, sessions: BoardSession[]) {
    for (const [id, previous] of unconfirmed) {
        const row = sessions.find((session) => session.id === id);
        if (row && row.accountScope !== previous.accountScope) unconfirmed.delete(id);
        else if (
            row &&
            sameCard(row, previous) &&
            !row.readOnly &&
            (row.version > previous.version ||
                row.inputVersion > previous.inputVersion ||
                row.sectionVersion !== previous.sectionVersion)
        )
            unconfirmed.delete(id);
    }
}

async function save(
    ports: EditsPorts,
    unconfirmed: Map<string, BoardSession>,
    session: BoardSession,
    state: string | null,
    note?: string,
) {
    const { announce, render } = ports;
    ports.setSaving(true);
    const panel = ports.movePanel();
    const operation = panel.start(session, state);
    let saved = false;
    try {
        const result = await persist(ports, session, state, note);
        operation.state = result.session.state;
        saved = true;
        const notice = result.titleEnabled
            ? result.titlePending
                ? `标题同步待重试。${result.titleError || ""}`
                : "标题已同步。"
            : "";
        announce(`状态已保存。${notice}`);
    } catch (error) {
        announce(
            "未收到保存确认，请核对实际位置后再操作。" +
                (error instanceof Error ? error.message : String(error)),
            true,
        );
        throw error;
    } finally {
        if (saved) unconfirmed.delete(session.id);
        else unconfirmed.set(session.id, session);
        panel.finish(operation, saved);
        ports.setSaving(false);
        render();
    }
}

function mountEditor(
    host: HTMLElement,
    fallback: () => HTMLElement,
    ports: EditsPorts,
    unconfirmed: Map<string, BoardSession>,
) {
    return mountCardEditor(host, {
        save(session, state, note) {
            if (ports.busy()) throw new Error("已有操作正在执行，请稍后保存。");
            return save(ports, unconfirmed, session, state, note);
        },
        async refresh(session) {
            const data = (await ports.request("/api/sessions")) as BoardSnapshot;
            const latest = data.sessions.find((row) => sameCard(row, session));
            if (!latest || !ports.canAdjust(latest))
                throw new Error("当前会话无法编辑，请关闭并刷新。");
            return latest;
        },
        closed(session) {
            ports.movePanel().resetEditor();
            ports.render();
            void ports.load();
            const root = host.getRootNode() as ShadowRoot | Document;
            const row = Array.from(root.querySelectorAll<HTMLElement>(".card")).find(
                (node) => node.getAttribute("data-id") === session.id,
            );
            const button = row?.querySelector<HTMLButtonElement>(
                '[data-action="adjust"]:not(:disabled)',
            );
            (button || fallback()).focus({ preventScroll: true });
        },
    });
}

export function mountBoardEdits(host: HTMLElement, fallback: () => HTMLElement, ports: EditsPorts) {
    const unconfirmed = new Map<string, BoardSession>();
    const editor = mountEditor(host, fallback, ports, unconfirmed);
    return {
        open(id: string, state?: string, current?: BoardSession) {
            if (ports.busy()) return;
            const { sessions, states } = ports.snapshot();
            const session = current || sessions.find((row) => row.id === id);
            if (!session) return;
            if (!ports.canAdjust(session)) {
                ports.announce("当前会话仅供查看，不能修改状态或备注。", true);
                return;
            }
            ports.movePanel().resetEditor();
            editor.open(session, states, state);
        },
        async move(session: BoardSession, state: string) {
            if (!ports.canAdjust(session)) return;
            try {
                await save(ports, unconfirmed, session, state);
            } catch {
                return;
            } finally {
                await ports.load();
            }
        },
        persist: (session: BoardSession, state: string | null, note?: string) =>
            persist(ports, session, state, note),
        reconcile: (sessions: BoardSession[]) => reconcile(unconfirmed, sessions),
        unconfirmed,
        unmount: editor.unmount,
    };
}
