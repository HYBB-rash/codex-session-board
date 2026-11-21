import { h, render } from "vue";
import BoardColumns from "./board-columns.vue";
import { createCardDrag } from "./board-drag";
import { createBoardLayout, type BoardElements } from "./board-layout";
import { sortColumn, type SortPreference, type SortMenuProps } from "./board-sorting";
import type { mountBoardArchive } from "./board-archive";
import type { PreviewSession } from "./reply-preview";

export interface BoardSession extends PreviewSession {
  state: string | null;
  note: string;
  threadId?: string;
  version: number;
  inputVersion: number;
  sectionVersion: string | null;
  readOnly?: boolean;
  manual?: boolean;
  reason?: string;
  placementReason?: string;
  remote?: boolean;
  archived?: boolean;
  canArchive?: boolean;
  archivePending?: boolean;
  presence?: string;
  catalogCurrent?: boolean;
  ordinary?: boolean;
  hostId?: string;
  projectId?: string | null;
  projectIssue?: string | null;
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
  moving?: { id: string; state: string | null; from: string | null } | null;
  unconfirmed: string[];
  archivePending: string[];
}

export interface BoardDependencies {
  canSort: SortMenuProps["canChange"];
  saveSort: SortMenuProps["save"];
  date: (value: number) => string;
  project: (session: BoardSession) => ProjectTag;
  actions: ReturnType<typeof mountBoardArchive>["cardActions"];
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
  update: (model: BoardModel, resetState?: string | null) => void;
}

export function mountBoard(elements: BoardElements, dependencies: BoardDependencies): BoardHandle {
  const { viewport } = elements;
  const layout = createBoardLayout(elements);
  const drag = createCardDrag(viewport, dependencies);
  const controls = { ...dependencies, drag };
  return {
    drag, sortColumn,
    update: (model, resetState) => layout.update(model, () => {
      const focusedSort = viewport.querySelector<HTMLButtonElement>(".column-sort:focus");
      render(h(BoardColumns, { model, dependencies: controls }), viewport);
      render(h(BoardColumns, { model, dependencies: controls, unplaced: true }), layout.unplacedCards);
      if (focusedSort?.isConnected && document.activeElement === document.body)
        focusedSort.focus({ preventScroll: true });
    }, resetState),
  };
}
