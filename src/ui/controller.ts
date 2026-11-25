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

class BoardPage {
    private readonly lifetime = new AbortController();
    private data: BoardSnapshot = {
        states: {},
        sessions: [],
        health: {},
        projectCatalog: { loaded: false, items: [] },
    };
    private saving = false;
    private creating = false;
    private readonly browse: ReturnType<typeof mountBoardBrowse>;
    private readonly reader: ReturnType<typeof mountBoardSessionReader>;
    private readonly reports: ReturnType<typeof createBoardHealthReports>;
    private readonly health: ReturnType<typeof mountBoardHealth>;
    private readonly sshSetup: ReturnType<typeof mountBoardSshSetup>;
    private readonly archive: ReturnType<typeof mountBoardArchive>;
    private readonly edits: ReturnType<typeof mountBoardEdits>;
    private readonly movePanel: ReturnType<typeof mountMoveFeedback>;
    private readonly newSession: ReturnType<typeof mountBoardNewSession>;
    private readonly newTarget: ReturnType<typeof mountBoardNewTarget>;
    private readonly assignment: ReturnType<typeof mountProjectAssignment>;
    private readonly board: ReturnType<typeof mountBoard>;
    private readonly refresh: ReturnType<typeof mountBoardRefresh>;
    private readonly stopPluginUpdate: () => void;
    private readonly poll: number;

    constructor(
        private readonly root: HTMLElement,
        private readonly transport: BoardRequest,
        replaceAccount: (snapshot: BoardSnapshot) => void,
        initial?: BoardSnapshot,
    ) {
        this.browse = this.mountBrowse();
        this.reader = this.mountReader();
        this.reports = this.createReports();
        this.health = this.mountHealth();
        this.sshSetup = this.mountSshSetup();
        this.archive = this.mountArchive();
        this.edits = this.mountEdits();
        this.movePanel = this.mountMovePanel();
        this.newSession = this.mountNewSession();
        this.newTarget = this.mountNewTarget();
        this.assignment = this.mountAssignment();
        this.board = this.mountColumns();
        this.stopPluginUpdate = mountPluginUpdate(
            this.byId("plugin-update"),
            this.byId("plugin-update-status"),
            this.request,
        );
        this.refresh = this.mountRefresh(replaceAccount, initial !== undefined);
        this.browse.filters.update(this.data);
        this.poll = window.setInterval(this.load, 4000);
        if (initial) {
            this.applySnapshot(initial);
            void this.archive.restore();
        } else {
            void this.load().then(() =>
                this.lifetime.signal.aborted ? undefined : this.archive.restore(),
            );
        }
    }

    private readonly byId = <Element extends HTMLElement>(id: string): Element => {
        const element = this.root.querySelector(`#${id}`);
        if (!element) throw new Error(`缺少看板元素 ${id}`);
        return element as Element;
    };

    private readonly snapshot = () => this.data;
    private readonly savingNow = () => this.saving;
    private readonly creatingNow = () => this.creating;
    private readonly setSaving = (value: boolean) => {
        this.saving = value;
    };
    private readonly busy = () => this.saving || this.creating;
    private readonly editorOpen = () => this.byId<HTMLDialogElement>("editor").open;
    private readonly dragActive = () => Boolean(this.board.drag.source);
    private readonly movingId = () => this.movePanel.moving?.id;
    private readonly invalidateSnapshot = () => {
        this.refresh.invalidate();
    };
    private readonly request: BoardRequest = async (path, body) => {
        this.lifetime.signal.throwIfAborted();
        if (body !== undefined) this.updateFilterLock();
        const result = await this.transport(path, body);
        this.lifetime.signal.throwIfAborted();
        return result;
    };

    private mountBrowse() {
        const { byId, snapshot, filtersLocked, render, announce } = this;
        return mountBoardBrowse(byId("board-filters"), byId("filter-scope"), {
            snapshot,
            locked: filtersLocked,
            changed: render,
            announce,
        });
    }

    private mountReader() {
        const { byId, request, announce, creatingNow, movingId } = this;
        return mountBoardSessionReader(byId("reply-preview"), {
            sessions: () => this.data.sessions,
            request,
            announce,
            dragSuppressesClick: () => this.board.drag.suppressesClick,
            savingWithoutMove: () => this.saving && !this.movePanel.moving,
            creating: creatingNow,
            movingId,
        });
    }

    private createReports() {
        const { snapshot } = this;
        const { filters, hostOptions, hostSessions, hostProjects, hostName } = this.browse;
        return createBoardHealthReports({
            snapshot,
            selectedHost: () => filters.state.host,
            selectedHostLabel: () => filters.view.hostLabel,
            hostOptions,
            hostSessions,
            hostProjects,
            hostName,
            date,
            readError: () => this.refresh.error,
        });
    }

    private mountHealth() {
        const { byId, request, load, announce } = this;
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
                reports: this.reports.healthReports,
                selectedHost: () => this.browse.filters.state.host,
                readState: () => this.refresh,
                date,
                request,
                load,
                announce,
            },
        );
    }

    private mountSshSetup() {
        const { byId, request, filtersLocked, updateFilterLock, announce, load } = this;
        return mountBoardSshSetup(byId("ssh-settings"), byId("ssh-setup"), {
            request,
            locked: filtersLocked,
            available: () =>
                this.data.health.nativeConnections === true ||
                this.data.health.sshAliasSetup === true,
            native: () => this.data.health.nativeConnections === true,
            updateFilterLock,
            announce,
            load,
        });
    }

    private mountArchive() {
        const {
            byId,
            snapshot,
            request,
            announce,
            render,
            load,
            busy,
            movingId,
            dragActive,
            editorOpen,
            setSaving,
            invalidateSnapshot,
        } = this;
        return mountBoardArchive(byId("archive-results"), byId("archive-status"), {
            snapshot,
            request,
            open: this.reader.openSession,
            date,
            announce,
            render,
            load,
            busy,
            movingId,
            dragActive,
            editorOpen,
            setSaving,
            invalidateSnapshot,
            persist: (session, state, note) => this.edits.persist(session, state, note),
        });
    }

    private mountEdits() {
        const {
            byId,
            snapshot,
            invalidateSnapshot,
            request,
            setSaving,
            busy,
            announce,
            render,
            load,
        } = this;
        return mountBoardEdits(byId("card-editor"), () => byId("host-filter"), {
            snapshot,
            invalidateSnapshot,
            request,
            movePanel: () => this.movePanel,
            setSaving,
            busy,
            canAdjust: (session) => this.archive.cardActions(session).canAdjust,
            announce,
            render,
            load,
        });
    }

    private mountMovePanel() {
        const { byId, announce, render, lifetime } = this;
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
                states: () => this.data.states,
                announce,
                render,
                signal: lifetime.signal,
            },
        );
    }

    private mountNewSession() {
        const {
            byId,
            snapshot,
            request,
            announce,
            render,
            load,
            updateFilterLock,
            invalidateSnapshot,
            savingNow,
            creatingNow,
            dragActive,
            editorOpen,
        } = this;
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
                    this.creating = value;
                },
                dragActive,
                editorOpen,
            },
        );
    }

    private mountNewTarget() {
        const {
            byId,
            snapshot,
            filtersLocked,
            savingNow,
            creatingNow,
            updateFilterLock,
            announce,
            load,
        } = this;
        const { filters, hostOptions, hostProjects, hostName, projectLabel } = this.browse;
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
                canCreateProject: this.newSession.canCreateProject,
                create: this.newSession.create,
                updateFilterLock,
                announce,
                load,
            },
        );
    }

    private mountAssignment() {
        const {
            byId,
            filtersLocked,
            savingNow,
            creatingNow,
            setSaving,
            updateFilterLock,
            invalidateSnapshot,
            request,
            announce,
            load,
        } = this;
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
                projects: () => this.data.projectCatalog.items,
                projectLabel: this.browse.projectLabel,
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

    private mountColumns() {
        const {
            canChangeColumnSort,
            snapshot,
            invalidateSnapshot,
            setSaving,
            request,
            announce,
            render,
            load,
            byId,
            editorOpen,
        } = this;
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
                project: this.browse.projectPresentation,
                actions: this.archive.cardActions,
                canSort: canChangeColumnSort,
                saveSort,
                canAssign: this.assignment.canAssign,
                preview: this.reader.openReply,
                edit: this.edits.open,
                assign: this.assignment.open,
                archive: this.archive.submitCard,
                archiveAll: this.archive.submit,
                editorOpen,
                announce,
                move: this.edits.move,
                refresh: render,
            },
        );
    }

    private readonly announce = (message: string, error = false) => {
        if (this.lifetime.signal.aborted) return;
        const feedback = this.byId("feedback");
        feedback.textContent = message;
        feedback.className = error ? "feedback error" : "feedback";
    };

    private readonly canChangeColumnSort = () =>
        !this.busy() && !this.dragActive() && !this.editorOpen();

    private readonly filtersLocked = () =>
        Boolean(
            this.dragActive() ||
                (this.saving && !this.movePanel.moving) ||
                this.creating ||
                this.editorOpen() ||
                this.newTarget.openDialog ||
                this.newSession.openDialog ||
                this.assignment.openDialog ||
                this.sshSetup.openDialog,
        );

    private readonly updateFilterLock = () => {
        if (!this.lifetime.signal.aborted) this.browse.filters.updateLock();
    };

    private columns(visible: BoardSession[]): BoardModel["columns"] {
        const selectedHost = this.browse.filters.state.host;
        const unread =
            !this.browse.hostSessions().length &&
            this.reports
                .healthReports()
                .some(
                    (host) =>
                        (selectedHost === "all" || host.key === selectedHost) && !host.lastRead,
                );
        return Object.entries(this.data.states).map(([state, label]) => {
            const { sessions, preference } = this.board.sortColumn(
                state,
                visible.filter((session) => session.state === state),
                this.data.columnSorts,
            );
            return {
                state,
                label,
                unread,
                sessions,
                preference,
                archiveCount: this.archive.count(sessions),
            };
        });
    }

    private boardModel(): BoardModel {
        const moving = this.movePanel.moving;
        const visible = this.browse
            .visibleSessions()
            .map((session) =>
                moving?.id === session.id ? { ...session, state: moving.state } : session,
            );
        const unplaced = this.data.fourColumns
            ? this.board.sortColumn(
                  null,
                  visible.filter((session) => session.state === null),
              ).sessions
            : [];
        const preview = this.reader.preview;
        return {
            states: this.data.states,
            columns: this.columns(visible),
            unplaced,
            scope: this.browse.filters.view.scope,
            legacyMode: this.data.legacyMode === true,
            saving: this.saving,
            creating: this.creating,
            get selectedId() {
                return preview.selectedId;
            },
            moving: visible.some((session) => session.id === moving?.id) ? moving : null,
            unconfirmed: [...this.edits.unconfirmed.keys()],
            archivePending: [...this.archive.pending],
        };
    }

    private readonly render = (resetState: string | null = null) => {
        if (this.lifetime.signal.aborted || this.dragActive() || this.editorOpen()) return;
        this.browse.filters.update(this.data);
        this.newTarget.update();
        this.board.update(this.boardModel(), resetState);
        if (this.data.legacyMode)
            this.byId("foot").textContent =
                "拖动卡片修改状态，备注随会话保存。“待归档”和“已完成”均为人工状态。新建会话仅打开输入页。";
        this.archive.updateStatus();
        this.health.render();
    };

    private readonly load = (): Promise<void> =>
        this.lifetime.signal.aborted ? Promise.resolve() : this.refresh.load();

    private applySnapshot(next: BoardSnapshot) {
        const { render } = this;
        this.data = next;
        this.archive.updateBoard(next);
        this.sshSetup.update();
        this.reader.preview.update(next.sessions, false);
        this.edits.reconcile(next.sessions);
        this.newSession.update(next);
        render();
    }

    private mountRefresh(replaceAccount: (snapshot: BoardSnapshot) => void, loaded: boolean) {
        const { byId, request, filtersLocked, announce, render, lifetime } = this;
        return mountBoardRefresh(
            byId("refresh"),
            {
                request,
                locked: () => this.saving || filtersLocked(),
                moving: () => Boolean(this.movePanel.moving),
                announce,
                render,
                renderHealth: () => {
                    if (!lifetime.signal.aborted) this.health.render();
                },
                currentScope: () => this.data.accountScope,
                received: (next, accountChanged) => {
                    if (lifetime.signal.aborted) return;
                    if (accountChanged) replaceAccount(next);
                    else this.applySnapshot(next);
                },
            },
            loaded,
        );
    }

    readonly unmount = () => {
        this.lifetime.abort();
        this.refresh.invalidate();
        window.clearInterval(this.poll);
        this.board.drag.cancel();
        this.stopPluginUpdate();
        this.reader.preview.unmount();
        this.archive.unmount();
        this.edits.unmount();
        this.board.unmount();
        this.browse.filters.unmount();
    };
}

export function startBoardPage(
    root: HTMLElement,
    transport: BoardRequest,
    replaceAccount: (snapshot: BoardSnapshot) => void,
    initial?: BoardSnapshot,
) {
    return new BoardPage(root, transport, replaceAccount, initial);
}
