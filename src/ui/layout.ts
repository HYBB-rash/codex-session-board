import type { BoardModel } from "./columns";

export interface BoardElements {
    viewport: HTMLElement;
    unplaced: HTMLDetailsElement;
    jump: HTMLElement;
    filters: HTMLElement;
}

function requiredElement<Element extends HTMLElement = HTMLElement>(
    parent: ParentNode,
    selector: string,
): Element {
    const element = parent.querySelector<Element>(selector);
    if (!element) throw new Error(`缺少看板元素 ${selector}`);
    return element;
}

function captureScroll(cards: HTMLElement) {
    const top = cards.scrollTop;
    const bounds = cards.getBoundingClientRect();
    const anchor =
        top > 0
            ? [...cards.querySelectorAll<HTMLElement>(".card")].find(
                  (card) => card.getBoundingClientRect().bottom > bounds.top,
              )
            : undefined;
    return {
        top,
        id: anchor?.getAttribute("data-id"),
        offset: anchor ? anchor.getBoundingClientRect().top - bounds.top : 0,
    };
}

function restoreScroll(cards: HTMLElement, position?: ReturnType<typeof captureScroll>) {
    cards.scrollTop = position?.top ?? 0;
    if (!position?.id) return;
    const anchor = [...cards.querySelectorAll<HTMLElement>(".card")].find(
        (card) => card.getAttribute("data-id") === position.id,
    );
    if (anchor)
        cards.scrollTop +=
            anchor.getBoundingClientRect().top -
            cards.getBoundingClientRect().top -
            position.offset;
}

function isDisabled(element: HTMLElement | undefined) {
    return element instanceof HTMLButtonElement && element.disabled;
}

function updateJumps(jump: HTMLElement, viewport: HTMLElement, columns: HTMLElement[]) {
    const buttons = [...jump.querySelectorAll<HTMLButtonElement>("button")];
    const next = columns.map((column) => {
        const state = column.getAttribute("data-state");
        const button =
            buttons.find((item) => item.getAttribute("data-state") === state) ??
            document.createElement("button");
        if (state !== null) button.setAttribute("data-state", state);
        else button.removeAttribute("data-state");
        const label = `${column.getAttribute("aria-label")} ${requiredElement(column, ".column-count").textContent}`;
        if (button.textContent !== label) button.textContent = label;
        button.onclick = () => {
            const board = requiredElement(viewport, ".board");
            viewport.scrollTo({
                left: column.offsetLeft - board.offsetLeft,
                behavior: "smooth",
            });
        };
        return button;
    });
    if (next.length !== buttons.length || next.some((button, index) => buttons[index] !== button))
        jump.replaceChildren(...next);
}

function captureCardFocus(root: ShadowRoot | Document) {
    const active = root.activeElement;
    if (!(active instanceof HTMLElement)) return;
    const card = active.closest(".card");
    if (!card) return;
    return {
        active,
        id: card.getAttribute("data-id"),
        state: card.closest(".column")?.getAttribute("data-state"),
        action:
            active.getAttribute("data-action") ||
            (active.closest(".project-tag") ? "project" : "preview"),
    };
}

function restoreCardFocus(
    focus: ReturnType<typeof captureCardFocus>,
    root: ShadowRoot | Document,
    cards: HTMLElement[],
    columns: HTMLElement[],
    fallback: () => HTMLElement,
) {
    if (!focus || (focus.active === root.activeElement && !isDisabled(focus.active))) return;
    const card = cards.find((item) => item.getAttribute("data-id") === focus.id);
    const selector =
        focus.action === "project" ? ".project-tag summary" : `[data-action="${focus.action}"]`;
    const target = card?.querySelector<HTMLElement>(selector);
    const previousColumn = columns.find(
        (column) => column.getAttribute("data-state") === focus.state,
    );
    const sort = previousColumn?.querySelector<HTMLElement>(".column-sort");
    (target && !isDisabled(target) ? target : (sort ?? fallback())).focus({ preventScroll: true });
}

function captureExpanded(cards: HTMLElement[]) {
    return new Map(
        cards.map((card) => [
            card.getAttribute("data-id"),
            {
                project: requiredElement<HTMLDetailsElement>(card, ".project-tag").open,
                note: Boolean(card.querySelector(".note-details[open]")),
            },
        ]),
    );
}

function restoreExpanded(cards: HTMLElement[], expanded: ReturnType<typeof captureExpanded>) {
    for (const card of cards) {
        const previous = expanded.get(card.getAttribute("data-id"));
        if (!previous) continue;
        requiredElement<HTMLDetailsElement>(card, ".project-tag").open = previous.project;
        const note = card.querySelector<HTMLDetailsElement>(".note-details");
        if (note) {
            note.open = previous.note;
            if (note.open) note.classList.add("expandable");
        }
    }
}

export function createBoardLayout({ viewport, unplaced, jump, filters }: BoardElements) {
    viewport.tabIndex = 0;
    const root = viewport.getRootNode() as ShadowRoot | Document;
    const unplacedCards = requiredElement(unplaced, ".cards");
    const unplacedSummary = requiredElement(unplaced, "summary");
    const columns = () => [...viewport.querySelectorAll<HTMLElement>(".column")];
    const cards = () => [
        ...viewport.querySelectorAll<HTMLElement>(".card"),
        ...unplacedCards.querySelectorAll<HTMLElement>(".card"),
    ];
    const columnCards = (column: HTMLElement) => requiredElement(column, ".cards");
    const fallback = () =>
        unplaced.hidden ? requiredElement(filters, "#host-filter") : unplacedSummary;

    function update(model: BoardModel, render: () => void, resetState?: string | null) {
        const scroll = new Map(
            columns().map((column) => {
                const state = column.getAttribute("data-state");
                return [
                    state,
                    (state ?? undefined) === resetState
                        ? undefined
                        : captureScroll(columnCards(column)),
                ];
            }),
        );
        const unplacedScroll = captureScroll(unplacedCards);
        const focus = captureCardFocus(root);
        const expanded = captureExpanded(cards());
        unplaced.hidden = !model.unplaced.length && model.moving?.from !== null;
        unplacedSummary.textContent = `未归列会话 ${model.unplaced.length + Number(model.moving?.from === null)}`;
        render();
        restoreScroll(unplacedCards, unplacedScroll);
        const nextColumns = columns();
        const nextCards = cards();
        updateJumps(jump, viewport, nextColumns);
        restoreExpanded(nextCards, expanded);
        restoreCardFocus(focus, root, nextCards, nextColumns, fallback);
        for (const column of nextColumns)
            restoreScroll(columnCards(column), scroll.get(column.getAttribute("data-state")));
    }
    return { update, unplacedCards };
}
