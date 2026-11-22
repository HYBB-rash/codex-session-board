import { mountBoard, type BoardSession } from "./board-view";
import { mountBoardArchive } from "./board-archive";
import { mountBoardBrowse, type CatalogProject, type HostIdentity } from "./board-browse";
import { createBoardHealthReports, type BoardHealth } from "./board-health-report";
import { mountBoardHealth } from "./board-health";
import { mountPluginUpdate } from "./board-plugin-update";
import { mountBoardSshSetup } from "./board-ssh-setup";
import { mountBoardSessionReader } from "./board-session-reader";
import { mountMoveFeedback } from "./board-move-feedback";
import { mountBoardEdits } from "./board-edits";
import { mountBoardRefresh } from "./board-refresh";
import { mountProjectAssignment } from "./board-project-assignment";
import { mountBoardNewSession } from "./board-new-session";
import { mountBoardNewTarget } from "./board-new-target";
import { createColumnSortSave, type SortPreference } from "./board-sorting";

export interface BoardSnapshot {
  states: Record<string, string>;
  sessions: BoardSession[];
  health: BoardHealth;
  projectCatalog: {
    loaded: boolean; items: CatalogProject[]; error?: string | null; remoteHosts?: HostIdentity[];
    cached?: boolean; incomplete?: boolean; pending?: boolean;
  };
  legacyMode?: boolean;
  fourColumns?: boolean;
  pendingDrafts?: { project: string; requestId: string }[];
  columnSorts?: Record<string, SortPreference>;
}

export type BoardRequest = (path: string, body?: object) => Promise<unknown>;

export function mountBoardPage(root: ShadowRoot | Document, transport: BoardRequest) {
function byId<Element extends HTMLElement>(id: string): Element {
  const element = root.getElementById(id);
  if (!element) throw new Error(`缺少看板元素 ${id}`);
  return element as Element;
}

const lifetime = new AbortController();
let data: BoardSnapshot = { states: {}, sessions: [], health: {}, projectCatalog: { loaded: false, items: [] } };
let saving = false;
let creating = false;
const snapshot = () => data;
const savingNow = () => saving;
const creatingNow = () => creating;
const setSaving = (value: boolean) => { saving = value; };
const busy = () => saving || creating;
const editorOpen = () => byId<HTMLDialogElement>("editor").open;
const dragActive = (): boolean => Boolean(board.drag.source);
const movingId = (): string | undefined => movePanel.moving?.id;
const invalidateSnapshot = (): void => { refresh.invalidate(); };
const request: BoardRequest = async (path, body) => {
  lifetime.signal.throwIfAborted();
  if (body !== undefined) updateFilterLock();
  return transport(path, body);
};
const { filters, hostOptions, hostProjects, hostSessions, hostName, projectLabel, projectPresentation, visibleSessions } =
  mountBoardBrowse(byId("board-filters"), byId("filter-scope"), { snapshot, locked: filtersLocked, changed: render, announce });
const sessionReader = mountBoardSessionReader(byId("reply-preview"), {
  sessions: () => data.sessions, request, announce, dragSuppressesClick: (): boolean => board.drag.suppressesClick,
  savingWithoutMove: (): boolean => saving && !movePanel.moving, creating: creatingNow, movingId,
});
const { preview: replyPreview, openReply, openSession } = sessionReader;
const date = (value?: number) => value ? new Date(value * 1000).toLocaleString("zh-CN",
  { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "尚未读取";
const { hostHealth, healthReports } = createBoardHealthReports({
  snapshot, selectedHost: () => filters.state.host, selectedHostLabel: () => filters.view.hostLabel,
  hostOptions, hostSessions, hostProjects, hostName, date, readError: (): string | null => refresh.error,
});
const connectionHealth = mountBoardHealth({
  banner: byId("health"), summary: byId("health-summary"), more: byId("health-more"),
  detail: byId("health-details"), list: byId("health-detail-list"), close: byId("health-close"),
  recheck: byId("health-recheck"), detailHelp: byId("health-help"), connectionHelp: byId("connection-help"),
  recovery: byId("connection-recovery"), recoveryClose: byId("connection-close"), recoveryRecheck: byId("connection-recheck"),
}, {
  reports: healthReports, selectedHost: () => filters.state.host, readState: (): ReturnType<typeof mountBoardRefresh> => refresh,
  date, request, load, announce,
});
const sshSetup = mountBoardSshSetup(byId("ssh-settings"), byId("ssh-setup"), {
  request, locked: filtersLocked, available: () => data.health.nativeConnections === true || data.health.sshAliasSetup === true,
  native: () => data.health.nativeConnections === true, updateFilterLock, announce, load,
});
const archive = mountBoardArchive(byId("archive-results"), byId("archive-status"), {
  snapshot, request, open: openSession, date, announce, render, load, busy, movingId, dragActive, editorOpen, setSaving,
  invalidateSnapshot, persist: (session, state, note): Promise<unknown> => edits.persist(session, state, note),
});
const edits = mountBoardEdits(byId("card-editor"), () => byId("host-filter"), {
  snapshot, invalidateSnapshot, request, movePanel: (): ReturnType<typeof mountMoveFeedback> => movePanel, setSaving, busy,
  canAdjust: (session): boolean => archive.cardActions(session).canAdjust, announce, render, load,
});
const { open: openEditor, move: moveCard, unconfirmed: unconfirmedMoves } = edits;
const movePanel = mountMoveFeedback({
  editor: byId("editor"), form: byId("edit-form"), slot: byId("move-slot"), panel: byId("move-feedback"),
  heading: byId("move-heading"), detail: byId("move-detail"), dismiss: byId("move-dismiss"), image: byId("move-dragon"),
}, { states: () => data.states, announce, render, signal: lifetime.signal });
const saveColumnSort = createColumnSortSave({
  canChange: canChangeColumnSort, snapshot, invalidateSnapshot, setSaving, request, announce, render, load,
});
const newSession = mountBoardNewSession({
  button: byId("new-session"), consentDialog: byId("native-consent"), consentForm: byId("native-consent-form"),
  consentTarget: byId("native-consent-target"), consentChanges: byId("native-consent-changes"), consentCancel: byId("native-consent-cancel"),
  recoveryDialog: byId("draft-recovery"), recoveryMessage: byId("draft-message"), recoveryLater: byId("draft-later"),
  recoveryRetry: byId("draft-retry"), recoveryRestart: byId("draft-restart"),
}, {
  snapshot, request, announce, render, load, updateFilterLock, invalidateSnapshot, saving: savingNow, creating: creatingNow,
  setCreating: value => { creating = value; }, dragActive, editorOpen,
});
const { create: createSession, canCreateProject } = newSession;
const newTarget = mountBoardNewTarget({
  button: byId("new-session"), dialog: byId("new-target"), form: byId("new-target-form"), host: byId("new-host"),
  project: byId("new-project"), submit: byId("new-target-open"), hint: byId("new-target-hint"), cancel: byId("new-target-cancel"),
}, {
  snapshot, scope: () => filters.state, hostOptions, hostProjects, hostName, projectLabel, locked: filtersLocked,
  saving: savingNow, creating: creatingNow, canCreateProject, create: createSession, updateFilterLock, announce, load,
});
const assignment = mountProjectAssignment({
  dialog: byId("project-assignment"), form: byId("assignment-form"), thread: byId("assignment-thread"),
  project: byId("assignment-project"), error: byId("assignment-error"), cancel: byId("assignment-cancel"), save: byId("assignment-save"),
}, {
  projects: () => data.projectCatalog.items, projectLabel, locked: filtersLocked, saving: savingNow, creating: creatingNow,
  setSaving, updateFilterLock, invalidateSnapshot, request, announce, load,
});
const board = mountBoard({
  viewport: byId("board-viewport"), unplaced: byId("unplaced"), jump: byId("jump"), filters: byId("board-filters"),
}, {
  date, project: projectPresentation, actions: archive.cardActions, canSort: canChangeColumnSort, saveSort: saveColumnSort,
  canAssign: assignment.canAssign, preview: openReply, edit: openEditor, assign: assignment.open,
  archive: archive.submitCard, archiveAll: archive.submit, editorOpen, announce, move: moveCard, refresh: render,
});

function announce(message: string, error = false) {
  const feedback = byId("feedback");
  feedback.textContent = message;
  feedback.className = error ? "feedback error" : "feedback";
}

function canChangeColumnSort() {
  return !busy() && !dragActive() && !editorOpen();
}

function filtersLocked() {
  return Boolean(dragActive() || saving && !movePanel.moving || creating || editorOpen() ||
    newTarget.openDialog || newSession.openDialog || assignment.openDialog || sshSetup.openDialog);
}

function updateFilterLock() {
  filters.updateLock();
}

function render(resetState: string | null = null) {
  if (dragActive() || editorOpen()) return;
  filters.update(data);
  newTarget.update();
  const moving = movePanel.moving;
  const visible = visibleSessions().map(session => moving?.id === session.id ? { ...session, state: moving.state } : session);
  const unplaced = data.fourColumns ? board.sortColumn(null, visible.filter(session => session.state === null)).sessions : [];
  const visibleMove = visible.some(session => session.id === moving?.id) ? moving : null;
  const selectedHost = filters.state.host;
  const unread = !hostSessions().length && healthReports().some(host => (selectedHost === "all" || host.key === selectedHost) && !host.lastRead);
  const columns = Object.entries(data.states).map(([state, label]) => {
    const { sessions, preference } = board.sortColumn(state, visible.filter(session => session.state === state), data.columnSorts);
    return { state, label, unread, sessions, preference, archiveCount: archive.count(sessions) };
  });
  board.update({
    states: data.states, columns, unplaced, scope: filters.view.scope, legacyMode: data.legacyMode === true, saving, creating,
    get selectedId() { return replyPreview.selectedId; },
    moving: visibleMove, unconfirmed: [...unconfirmedMoves.keys()], archivePending: [...archive.pending],
  }, resetState);
  if (data.legacyMode) byId("foot").textContent =
    "拖动卡片修改状态，备注随会话保存。“待归档”和“已完成”均为人工状态。新建会话仅打开输入页。";
  archive.updateStatus();
  connectionHealth.render();
}

function load(): Promise<void> {
  return refresh.load();
}

const stopPluginUpdate = mountPluginUpdate(byId("plugin-update"), byId("plugin-update-status"), request);
const refresh = mountBoardRefresh(byId("refresh"), {
  request, locked: () => saving || filtersLocked(), moving: () => Boolean(movePanel.moving), announce, render,
  renderHealth: connectionHealth.render,
  received(next) {
    if (lifetime.signal.aborted) return;
    data = next;
    archive.updateBoard(data);
    sshSetup.update();
    replyPreview.update(data.sessions, false);
    edits.reconcile(data.sessions);
    newSession.update(data);
    render();
  },
});

const boardPage = {
  request, load, refresh, filtersLocked, render, archive, openEditor, openSession, movePanel, moveCard, unconfirmedMoves,
  visibleSessions, createSession, canCreateProject, projectLabel, hostHealth, hostOptions, connectionHealth, sessionReader, sshSetup,
  board, newSession,
  get data() { return data; }, set data(value: BoardSnapshot) { data = value; },
  get saving() { return saving; }, set saving(value: boolean) { saving = value; },
  get creating() { return creating; }, set creating(value: boolean) { creating = value; },
  unmount() {
    lifetime.abort();
    window.clearInterval(poll);
    stopPluginUpdate();
    sessionReader.preview.unmount();
    archive.unmount();
    edits.unmount();
    board.unmount();
  },
};

filters.update(data);
void load().then(() => { if (!lifetime.signal.aborted) return archive.restore(); });
const poll = window.setInterval(load, 4000);
return boardPage;
}
