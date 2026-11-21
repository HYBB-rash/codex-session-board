import { h, render } from "vue";
import BoardColumns from "./board-columns.vue";
import { createCardDrag } from "./board-drag";
import { sortColumn, type SortPreference, type SortMenuProps } from "./board-sorting";
import type { PreviewSession } from "./reply-preview";

export interface BoardSession extends PreviewSession {
  state: string | null;
  note?: string;
  manual?: boolean;
  reason?: string;
  placementReason?: string;
  remote?: boolean;
  archived?: boolean;
  presence?: string;
  catalogCurrent?: boolean;
  hostId?: string;
  projectId?: string;
  projectIssue?: string;
  cwd?: string;
  source?: string;
  followUp?: boolean;
  observationError?: string;
  observationWarning?: string;
  titleSyncPending?: boolean;
  titleSyncError?: string;
}

export interface ProjectTag {
  label: string;
  summary: string;
  pending: boolean;
  info: string[];
}

export interface BoardModel {
  states: Record<string, string>;
  columns: { state: string; label: string; sessions: BoardSession[]; preference: SortPreference; unread: boolean; archiveCount: number }[];
  scope: string;
  unplaced: BoardSession[];
  legacyMode: boolean;
  saving: boolean;
  creating: boolean;
  selectedId?: string;
  moving?: { id: string; state: string; from: string | null } | null;
  unconfirmed: string[];
  archivePending: string[];
}

export interface BoardDependencies {
  canSort: SortMenuProps["canChange"];
  saveSort: SortMenuProps["save"];
  date: (value: number) => string;
  project: (session: BoardSession) => ProjectTag;
  actions: (session: BoardSession) => {
    readOnly: boolean; canAdjust: boolean; pendingArchive: boolean;
    archive: "archive" | "confirm" | null; canSubmit: boolean;
  };
  canAssign: (session: BoardSession) => boolean;
  preview: (session: BoardSession, focus?: boolean) => void;
  edit: (id: string, state?: string, session?: BoardSession) => void;
  assign: (session: BoardSession) => void;
  archive: (session: BoardSession) => void;
  archiveAll: (sessions: BoardSession[]) => void;
  editorOpen: () => boolean;
  refresh: () => void;
  announce: (message: string) => void;
  move: (session: BoardSession, state: string) => void;
}

export type BoardControls = BoardDependencies & { drag: ReturnType<typeof createCardDrag> };

export interface BoardHandle {
  sortColumn: typeof sortColumn;
  drag: ReturnType<typeof createCardDrag>;
  update: (model: BoardModel) => void;
}

declare global {
  interface Window {
    mountBoard: (host: HTMLElement, unplaced: HTMLElement, dependencies: BoardDependencies) => BoardHandle;
  }
}

window.mountBoard = (host, unplaced, dependencies) => {
  const drag = createCardDrag(host, dependencies);
  const controls = { ...dependencies, drag };
  return { drag, sortColumn, update: model => {
    const focusedSort = host.querySelector<HTMLButtonElement>(".column-sort:focus");
    render(h(BoardColumns, { model, dependencies: controls }), host);
    render(h(BoardColumns, { model, dependencies: controls, unplaced: true }), unplaced);
    if (focusedSort?.isConnected && document.activeElement === document.body)
      focusedSort.focus({ preventScroll: true });
  } };
};
