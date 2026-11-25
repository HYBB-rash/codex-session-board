import { mountBoardEdits } from "../../edit/edits";
import { mountMoveFeedback } from "../../edit/move-feedback";
import { mountBoardSessionReader } from "../../preview/reader";
import { mountBoardArchive } from "./archive/controls";
import { type CatalogProject, type HostIdentity, mountBoardBrowse } from "./browse/scope";
import { type BoardModel, mountBoard } from "./columns";
import { mountBoardNewSession } from "./connection/new/session";
import { mountBoardNewTarget } from "./connection/new/target";
import { mountProjectAssignment } from "./connection/project-assignment";
import { mountBoardSshSetup } from "./connection/ssh-setup";
import { mountBoardHealth } from "./health/banner";
import { type BoardHealth, createBoardHealthReports } from "./health/report";
import { mountPluginUpdate } from "./plugin-update";
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

    private byId<Element extends HTMLElement>(id: string): Element {
        const element = this.root.querySelector(`#${id}`);
        if (!element) throw new Error(`缺少看板元素 ${id}`);
        return element as Element;
    }

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
        return mountBoardBrowse(this.byId("board-filters"), this.byId("filter-scope"), {
            snapshot: this.snapshot,
            locked: this.filtersLocked,
            changed: this.render,
            announce: this.announce,
        });
    }

    private mountReader() {
        return mountBoardSessionReader(this.byId("reply-preview"), {
            sessions: () => this.data.sessions,
            request: this.request,
            announce: this.announce,
            dragSuppressesClick: () => this.board.drag.suppressesClick,
            savingWithoutMove: () => this.saving && !this.movePanel.moving,
            creating: this.creatingNow,
            movingId: this.movingId,
        });
    }

    private createReports() {
        const { filters, hostOptions, hostSessions, hostProjects, hostName } = this.browse;
        return createBoardHealthReports({
            snapshot: this.snapshot,
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
        return mountBoardHealth(
            {
                banner: this.byId("health"),
                summary: this.byId("health-summary"),
                more: this.byId("health-more"),
                detail: this.byId("health-details"),
                list: this.byId("health-detail-list"),
                close: this.byId("health-close"),
                recheck: this.byId("health-recheck"),
                detailHelp: this.byId("health-help"),
                connectionHelp: this.byId("connection-help"),
                recovery: this.byId("connection-recovery"),
                recoveryClose: this.byId("connection-close"),
                recoveryRecheck: this.byId("connection-recheck"),
            },
            {
                reports: this.reports.healthReports,
                selectedHost: () => this.browse.filters.state.host,
                readState: () => this.refresh,
                date,
                request: this.request,
                load: this.load,
                announce: this.announce,
            },
        );
    }

    private mountSshSetup() {
        return mountBoardSshSetup(this.byId("ssh-settings"), this.byId("ssh-setup"), {
            request: this.request,
            locked: this.filtersLocked,
            available: () =>
                this.data.health.nativeConnections === true ||
                this.data.health.sshAliasSetup === true,
            native: () => this.data.health.nativeConnections === true,
            updateFilterLock: this.updateFilterLock,
            announce: this.announce,
            load: this.load,
        });
    }

    private mountArchive() {
        return mountBoardArchive(this.byId("archive-results"), this.byId("archive-status"), {
            snapshot: this.snapshot,
            request: this.request,
            open: this.reader.openSession,
            date,
            announce: this.announce,
            render: this.render,
            load: this.load,
            busy: this.busy,
            movingId: this.movingId,
            dragActive: this.dragActive,
            editorOpen: this.editorOpen,
            setSaving: this.setSaving,
            invalidateSnapshot: this.invalidateSnapshot,
            persist: (session, state, note) => this.edits.persist(session, state, note),
        });
    }

    private mountEdits() {
        return mountBoardEdits(this.byId("card-editor"), () => this.byId("host-filter"), {
            snapshot: this.snapshot,
            invalidateSnapshot: this.invalidateSnapshot,
            request: this.request,
            movePanel: () => this.movePanel,
            setSaving: this.setSaving,
            busy: this.busy,
            canAdjust: (session) => this.archive.cardActions(session).canAdjust,
            announce: this.announce,
            render: this.render,
            load: this.load,
        });
    }

    private mountMovePanel() {
        return mountMoveFeedback(
            {
                editor: this.byId("editor"),
                form: this.byId("edit-form"),
                slot: this.byId("move-slot"),
                panel: this.byId("move-feedback"),
                heading: this.byId("move-heading"),
                detail: this.byId("move-detail"),
                dismiss: this.byId("move-dismiss"),
                image: this.byId("move-dragon"),
            },
            {
                states: () => this.data.states,
                announce: this.announce,
                render: this.render,
                signal: this.lifetime.signal,
            },
        );
    }

    private mountNewSession() {
        return mountBoardNewSession(
            {
                button: this.byId("new-session"),
                consentDialog: this.byId("native-consent"),
                consentForm: this.byId("native-consent-form"),
                consentTarget: this.byId("native-consent-target"),
                consentChanges: this.byId("native-consent-changes"),
                consentCancel: this.byId("native-consent-cancel"),
                recoveryDialog: this.byId("draft-recovery"),
                recoveryMessage: this.byId("draft-message"),
                recoveryLater: this.byId("draft-later"),
                recoveryRetry: this.byId("draft-retry"),
                recoveryRestart: this.byId("draft-restart"),
            },
            {
                snapshot: this.snapshot,
                request: this.request,
                announce: this.announce,
                render: this.render,
                load: this.load,
                updateFilterLock: this.updateFilterLock,
                invalidateSnapshot: this.invalidateSnapshot,
                saving: this.savingNow,
                creating: this.creatingNow,
                setCreating: (value) => {
                    this.creating = value;
                },
                dragActive: this.dragActive,
                editorOpen: this.editorOpen,
            },
        );
    }

    private mountNewTarget() {
        const { filters, hostOptions, hostProjects, hostName, projectLabel } = this.browse;
        return mountBoardNewTarget(
            {
                button: this.byId("new-session"),
                dialog: this.byId("new-target"),
                form: this.byId("new-target-form"),
                host: this.byId("new-host"),
                project: this.byId("new-project"),
                submit: this.byId("new-target-open"),
                hint: this.byId("new-target-hint"),
                cancel: this.byId("new-target-cancel"),
            },
            {
                snapshot: this.snapshot,
                scope: () => filters.state,
                hostOptions,
                hostProjects,
                hostName,
                projectLabel,
                locked: this.filtersLocked,
                saving: this.savingNow,
                creating: this.creatingNow,
                canCreateProject: this.newSession.canCreateProject,
                create: this.newSession.create,
                updateFilterLock: this.updateFilterLock,
                announce: this.announce,
                load: this.load,
            },
        );
    }

    private mountAssignment() {
        return mountProjectAssignment(
            {
                dialog: this.byId("project-assignment"),
                form: this.byId("assignment-form"),
                thread: this.byId("assignment-thread"),
                project: this.byId("assignment-project"),
                error: this.byId("assignment-error"),
                cancel: this.byId("assignment-cancel"),
                save: this.byId("assignment-save"),
            },
            {
                projects: () => this.data.projectCatalog.items,
                projectLabel: this.browse.projectLabel,
                locked: this.filtersLocked,
                saving: this.savingNow,
                creating: this.creatingNow,
                setSaving: this.setSaving,
                updateFilterLock: this.updateFilterLock,
                invalidateSnapshot: this.invalidateSnapshot,
                request: this.request,
                announce: this.announce,
                load: this.load,
            },
        );
    }

    private mountColumns() {
        const saveSort = createColumnSortSave({
            canChange: this.canChangeColumnSort,
            snapshot: this.snapshot,
            invalidateSnapshot: this.invalidateSnapshot,
            setSaving: this.setSaving,
            request: this.request,
            announce: this.announce,
            render: this.render,
            load: this.load,
        });
        return mountBoard(
            {
                viewport: this.byId("board-viewport"),
                unplaced: this.byId("unplaced"),
                jump: this.byId("jump"),
                filters: this.byId("board-filters"),
            },
            {
                date,
                project: this.browse.projectPresentation,
                actions: this.archive.cardActions,
                canSort: this.canChangeColumnSort,
                saveSort,
                canAssign: this.assignment.canAssign,
                preview: this.reader.openReply,
                edit: this.edits.open,
                assign: this.assignment.open,
                archive: this.archive.submitCard,
                archiveAll: this.archive.submit,
                editorOpen: this.editorOpen,
                announce: this.announce,
                move: this.edits.move,
                refresh: this.render,
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
        this.data = next;
        this.archive.updateBoard(next);
        this.sshSetup.update();
        this.reader.preview.update(next.sessions, false);
        this.edits.reconcile(next.sessions);
        this.newSession.update(next);
        this.render();
    }

    private mountRefresh(replaceAccount: (snapshot: BoardSnapshot) => void, loaded: boolean) {
        return mountBoardRefresh(
            this.byId("refresh"),
            {
                request: this.request,
                locked: () => this.saving || this.filtersLocked(),
                moving: () => Boolean(this.movePanel.moving),
                announce: this.announce,
                render: this.render,
                renderHealth: () => {
                    if (!this.lifetime.signal.aborted) this.health.render();
                },
                currentScope: () => this.data.accountScope,
                received: (next, accountChanged) => {
                    if (this.lifetime.signal.aborted) return;
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
