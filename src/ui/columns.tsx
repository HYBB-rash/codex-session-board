import { render } from "vue";
import type { mountBoardArchive } from "./archive/controls";
import { createCardDrag } from "./drag";
import { type BoardElements, createBoardLayout } from "./layout";
import BoardColumns from "./SessionColumns.vue";
import type { BoardSession } from "./session";
import type { ProjectTag } from "./shared/card/details";
import { type SortMenuProps, type SortPreference, sortColumn } from "./sorting";

export interface BoardModel {
    states: Record<string, string>;
    columns: {
        state: string;
        label: string;
        sessions: BoardSession[];
        preference: SortPreference;
        unread: boolean;
    }[];
    scope: string;
    unplaced: BoardSession[];
    legacyMode: boolean;
    saving: boolean;
    creating: boolean;
    selectedId: string | undefined;
    moving?: { id: string; state: string | null; from: string | null } | null;
    unconfirmed: string[];
}

export interface BoardDependencies {
    canSort: SortMenuProps["canChange"];
    saveSort: SortMenuProps["save"];
    date: (value: number) => string;
    project: (session: BoardSession) => ProjectTag;
    actions: ReturnType<typeof mountBoardArchive>["cardActions"];
    canAssign: (session: BoardSession) => boolean;
    nativeConversation: boolean;
    openCard: (session: BoardSession, focus?: boolean, origin?: HTMLElement) => void;
    edit: (id: string, state?: string, session?: BoardSession) => void;
    assign: (session: BoardSession) => void;
    archive: (session: BoardSession) => void;
    restorePlacement: (id: string) => void;
    editorOpen: () => boolean;
    refresh: () => void;
    announce: (message: string, error?: boolean) => void;
    move: (session: BoardSession, state: string) => void;
}

export type BoardControls = BoardDependencies & { drag: ReturnType<typeof createCardDrag> };

export interface BoardHandle {
    unmount(): void;
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
        drag,
        sortColumn,
        unmount() {
            render(null, viewport);
            render(null, layout.unplacedCards);
        },
        update: (model, resetState) =>
            layout.update(
                model,
                () => {
                    const focusedSort =
                        viewport.querySelector<HTMLButtonElement>(".column-sort:focus");
                    render(<BoardColumns model={model} dependencies={controls} />, viewport);
                    render(
                        <BoardColumns model={model} dependencies={controls} unplaced />,
                        layout.unplacedCards,
                    );
                    if (focusedSort?.isConnected && document.activeElement === document.body)
                        focusedSort.focus({ preventScroll: true });
                },
                resetState,
            ),
    };
}
