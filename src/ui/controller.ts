import { mountBoardArchive } from "./archive/controls";
import { type CatalogProject, type HostIdentity, mountBoardBrowse } from "./browse/scope";
import { type BoardModel, mountBoard } from "./columns";
import { mountBoardNewSession } from "./connection/new/session";
import { mountBoardNewTarget } from "./connection/new/target";
import { mountProjectAssignment } from "./connection/project-assignment";
import { mountBoardSshSetup } from "./connection/ssh-setup";
import { mountBoardEdits } from "./edit/edits";
import { mountMoveFeedback } from "./edit/move-feedback";
import { mountBoardHealth } from "./health/banner";
import { type BoardHealth, createBoardHealthReports } from "./health/report";
import { mountPluginUpdate } from "./plugin-update";
import { mountBoardSessionReader } from "./preview/reader";
import { mountBoardRefresh } from "./refresh";
import type { BoardSession } from "./session";
import { createColumnSortSave, type SortPreference } from "./sorting";

export interface BoardSnapshot {
    accountScope?: string;
    states: Record<string, string>;
    sessions: BoardSession[];
    archivedSessions?: BoardSession[];
    health: BoardHealth;
    projectCatalog: {
        loaded: boolean;
        items: CatalogProject[];
        error?: string | null;
        remoteHosts?: HostIdentity[];
        cached?: boolean;
        incomplete?: boolean;
        pending?: boolean;
    };
    legacyMode?: boolean;
    fourColumns?: boolean;
    pendingDrafts?: { project: string; requestId: string }[];
    columnSorts?: Record<string, SortPreference>;
}

export type BoardRequest = (path: string, body?: object) => Promise<unknown>;

export interface BoardConversation {
    open(id: string, origin?: HTMLElement): Promise<boolean>;
    close(): void;
}

function date(value?: number) {
    return value
        ? new Date(value * 1000).toLocaleString("zh-CN", {
              month: "numeric",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
          })
        : "尚未读取";
}

export function startBoardPage(
    root: HTMLElement,
    transport: BoardRequest,
    replaceAccount: (snapshot: BoardSnapshot) => void,
    initial?: BoardSnapshot,
    openConversation?: BoardConversation["open"],
) {
    const lifetime = new AbortController();
    let data: BoardSnapshot = {
        states: {},
        sessions: [],
        health: {},
        projectCatalog: { loaded: false, items: [] },
    };
    let saving = false;
    let creating = false;

    function byId<Element extends HTMLElement>(id: string): Element {
        const element = root.querySelector(`#${id}`);
        if (!element) throw new Error(`缺少看板元素 ${id}`);
        return element as Element;
    }

    const snapshot = () => data;
    const savingNow = () => saving;
    const creatingNow = () => creating;
    const setSaving = (value: boolean) => {
        saving = value;
    };
    const busy = () => saving || creating;
    const editorOpen = () => byId<HTMLDialogElement>("editor").open;
    const dragActive = (): boolean => Boolean(board.drag.source);
    const movingId = () => movePanel.moving?.id;
    const invalidateSnapshot = () => {
        refresh.invalidate();
    };
    const request: BoardRequest = async (path, body) => {
        lifetime.signal.throwIfAborted();
        if (body !== undefined) updateFilterLock();
        const result = await transport(path, body);
        lifetime.signal.throwIfAborted();
        return result;
    };

    function mountBrowse() {
        return mountBoardBrowse(byId("board-filters"), byId("filter-scope"), {
            snapshot,
            locked: filtersLocked,
            changed: render,
            announce,
        });
    }

    function mountReader() {
        return mountBoardSessionReader(byId("reply-preview"), {
            sessions: () => data.sessions,
            request,
            announce,
            dragSuppressesClick: () => board.drag.suppressesClick,
            savingWithoutMove: () => saving && !movePanel.moving,
            creating: creatingNow,
            movingId,
        });
    }

    function createReports() {
        const { filters, hostOptions, hostSessions, hostProjects, hostName } = browse;
        return createBoardHealthReports({
            snapshot,
            selectedHost: () => filters.state.host,
            selectedHostLabel: () => filters.view.hostLabel,
            hostOptions,
            hostSessions,
            hostProjects,
            hostName,
            date,
            readError: () => refresh.error,
        });
    }

    function mountHealth() {
        return mountBoardHealth(
            {
                banner: byId("health"),
                summary: byId("health-summary"),
                more: byId("health-more"),
                detail: byId("health-details"),
                list: byId("health-detail-list"),
                close: byId("health-close"),
                recheck: byId("health-recheck"),
                detailHelp: byId("health-help"),
                connectionHelp: byId("connection-help"),
                recovery: byId("connection-recovery"),
                recoveryClose: byId("connection-close"),
                recoveryRecheck: byId("connection-recheck"),
            },
            {
                reports: reports.healthReports,
                selectedHost: () => browse.filters.state.host,
                readState: () => refresh,
                date,
                request,
                load,
                announce,
            },
        );
    }

    function mountSshSetup() {
        return mountBoardSshSetup(byId("ssh-settings"), byId("ssh-setup"), {
            request,
            locked: filtersLocked,
            available: () =>
                data.health.nativeConnections === true || data.health.sshAliasSetup === true,
            native: () => data.health.nativeConnections === true,
            updateFilterLock,
            announce,
            load,
        });
    }

    function mountArchive(): ReturnType<typeof mountBoardArchive> {
        return mountBoardArchive(byId("archive-history"), {
            snapshot,
            request,
            project: browse.projectPresentation,
            date,
            announce,
            render,
            load,
            busy,
            movingId,
            dragActive,
            editorOpen,
            invalidateSnapshot,
        });
    }

    function mountEdits() {
        return mountBoardEdits(byId("card-editor"), () => byId("host-filter"), {
            snapshot,
            invalidateSnapshot,
            request,
            movePanel: () => movePanel,
            setSaving,
            busy,
            canAdjust: (session) => archive.cardActions(session).canAdjust,
            announce,
            render,
            load,
        });
    }

    function mountMovePanel() {
        return mountMoveFeedback(
            {
                editor: byId("editor"),
                form: byId("edit-form"),
                slot: byId("move-slot"),
                panel: byId("move-feedback"),
                heading: byId("move-heading"),
                detail: byId("move-detail"),
                dismiss: byId("move-dismiss"),
                image: byId("move-dragon"),
            },
            {
                states: () => data.states,
                announce,
                render,
                signal: lifetime.signal,
            },
        );
    }

    function mountNewSession() {
        return mountBoardNewSession(
            {
                button: byId("new-session"),
                consentDialog: byId("native-consent"),
                consentForm: byId("native-consent-form"),
                consentTarget: byId("native-consent-target"),
                consentChanges: byId("native-consent-changes"),
                consentCancel: byId("native-consent-cancel"),
                recoveryDialog: byId("draft-recovery"),
                recoveryMessage: byId("draft-message"),
                recoveryLater: byId("draft-later"),
                recoveryRetry: byId("draft-retry"),
                recoveryRestart: byId("draft-restart"),
            },
            {
                snapshot,
                request,
                announce,
                render,
                load,
                updateFilterLock,
                invalidateSnapshot,
                saving: savingNow,
                creating: creatingNow,
                setCreating: (value) => {
                    creating = value;
                },
                dragActive,
                editorOpen,
            },
        );
    }

    function mountNewTarget() {
        const { filters, hostOptions, hostProjects, hostName, projectLabel } = browse;
        return mountBoardNewTarget(
            {
                button: byId("new-session"),
                dialog: byId("new-target"),
                form: byId("new-target-form"),
                host: byId("new-host"),
                project: byId("new-project"),
                submit: byId("new-target-open"),
                hint: byId("new-target-hint"),
                cancel: byId("new-target-cancel"),
            },
            {
                snapshot,
                scope: () => filters.state,
                hostOptions,
                hostProjects,
                hostName,
                projectLabel,
                locked: filtersLocked,
                saving: savingNow,
                creating: creatingNow,
                canCreateProject: newSession.canCreateProject,
                create: newSession.create,
                updateFilterLock,
                announce,
                load,
            },
        );
    }

    function mountAssignment() {
        return mountProjectAssignment(
            {
                dialog: byId("project-assignment"),
                form: byId("assignment-form"),
                thread: byId("assignment-thread"),
                project: byId("assignment-project"),
                error: byId("assignment-error"),
                cancel: byId("assignment-cancel"),
                save: byId("assignment-save"),
            },
            {
                projects: () => data.projectCatalog.items,
                projectLabel: browse.projectLabel,
                locked: filtersLocked,
                saving: savingNow,
                creating: creatingNow,
                setSaving,
                updateFilterLock,
                invalidateSnapshot,
                request,
                announce,
                load,
            },
        );
    }

    function mountColumns(): ReturnType<typeof mountBoard> {
        const saveSort = createColumnSortSave({
            canChange: canChangeColumnSort,
            snapshot,
            invalidateSnapshot,
            setSaving,
            request,
            announce,
            render,
            load,
        });
        return mountBoard(
            {
                viewport: byId("board-viewport"),
                unplaced: byId("unplaced"),
                jump: byId("jump"),
                filters: byId("board-filters"),
            },
            {
                date,
                project: browse.projectPresentation,
                actions: archive.cardActions,
                canSort: canChangeColumnSort,
                saveSort,
                canAssign: assignment.canAssign,
                nativeConversation: openConversation !== undefined,
                openCard: openConversation
                    ? (session, _focus, origin) => {
                          if (!creating)
                              void reader.openSession(session.id, (id) =>
                                  openConversation(id, origin),
                              );
                      }
                    : reader.openReply,
                edit: edits.open,
                assign: assignment.open,
                archive: archive.submitCard,
                restorePlacement: archive.restorePlacement,
                editorOpen,
                announce,
                move: edits.move,
                refresh: render,
            },
        );
    }

    const announce = (message: string, error = false) => {
        if (lifetime.signal.aborted) return;
        const feedback = byId("feedback");
        feedback.textContent = message;
        feedback.className = error ? "feedback error" : "feedback";
    };
    const canChangeColumnSort = () => !busy() && !dragActive() && !editorOpen();
    const filtersLocked = (): boolean =>
        Boolean(
            dragActive() ||
                (saving && !movePanel.moving) ||
                creating ||
                editorOpen() ||
                newTarget.openDialog ||
                newSession.openDialog ||
                assignment.openDialog ||
                sshSetup.openDialog,
        );
    const updateFilterLock = () => {
        if (!lifetime.signal.aborted) browse.filters.updateLock();
    };

    function columns(visible: BoardSession[]): BoardModel["columns"] {
        const selectedHost = browse.filters.state.host;
        const unread =
            !browse.hostSessions().length &&
            reports
                .healthReports()
                .some(
                    (host) =>
                        (selectedHost === "all" || host.key === selectedHost) && !host.lastRead,
                );
        return Object.entries(data.states).map(([state, label]) => {
            const { sessions, preference } = board.sortColumn(
                state,
                visible.filter((session) => session.state === state),
                data.columnSorts,
            );
            return {
                state,
                label,
                unread,
                sessions,
                preference,
            };
        });
    }

    function boardModel(): BoardModel {
        const moving = movePanel.moving;
        const visible = browse
            .visibleSessions()
            .map((session) =>
                moving?.id === session.id ? { ...session, state: moving.state } : session,
            );
        const unplaced = data.fourColumns
            ? board.sortColumn(
                  null,
                  visible.filter((session) => session.state === null),
              ).sessions
            : [];
        const preview = reader.preview;
        return {
            states: data.states,
            columns: columns(visible),
            unplaced,
            scope: browse.filters.view.scope,
            legacyMode: data.legacyMode === true,
            saving,
            creating,
            get selectedId() {
                return preview.selectedId;
            },
            moving: visible.some((session) => session.id === moving?.id) ? moving : null,
            unconfirmed: [...edits.unconfirmed.keys()],
        };
    }

    const render = (resetState: string | null = null) => {
        if (lifetime.signal.aborted || dragActive() || editorOpen()) return;
        browse.filters.update(data);
        newTarget.update();
        board.update(boardModel(), resetState);
        health.render();
    };
    const load = (): Promise<void> =>
        lifetime.signal.aborted ? Promise.resolve() : refresh.load();

    function applySnapshot(next: BoardSnapshot) {
        data = next;
        archive.updateBoard(next);
        sshSetup.update();
        reader.preview.update(next.sessions, false);
        edits.reconcile(next.sessions);
        newSession.update(next);
        render();
    }

    function mountRefresh(replaceAccount: (snapshot: BoardSnapshot) => void, loaded: boolean) {
        return mountBoardRefresh(
            byId("refresh"),
            {
                request,
                locked: () => saving || filtersLocked(),
                moving: () => Boolean(movePanel.moving),
                announce,
                render,
                renderHealth: () => {
                    if (!lifetime.signal.aborted) health.render();
                },
                currentScope: () => data.accountScope,
                received: (next, accountChanged) => {
                    if (lifetime.signal.aborted) return;
                    if (accountChanged) replaceAccount(next);
                    else applySnapshot(next);
                },
            },
            loaded,
        );
    }

    const unmount = () => {
        lifetime.abort();
        refresh.invalidate();
        window.clearInterval(poll);
        board.drag.cancel();
        stopPluginUpdate();
        reader.preview.unmount();
        archive.unmount();
        edits.unmount();
        board.unmount();
        browse.filters.unmount();
    };

    const browse = mountBrowse();
    const reader = mountReader();
    const reports = createReports();
    const health = mountHealth();
    const sshSetup = mountSshSetup();
    const archive = mountArchive();
    const edits = mountEdits();
    const movePanel = mountMovePanel();
    const newSession = mountNewSession();
    const newTarget = mountNewTarget();
    const assignment = mountAssignment();
    const board = mountColumns();
    const stopPluginUpdate = mountPluginUpdate(
        byId("plugin-update"),
        byId("plugin-update-status"),
        request,
    );
    const refresh = mountRefresh(replaceAccount, initial !== undefined);
    browse.filters.update(data);
    const poll = window.setInterval(load, 4000);
    if (initial) applySnapshot(initial);
    else void load();

    return { unmount };
}
