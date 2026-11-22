import type { BoardModel } from "./board-view";

export interface BoardElements {
  viewport: HTMLElement;
  unplaced: HTMLDetailsElement;
  jump: HTMLElement;
  filters: HTMLElement;
}

function captureScroll(cards: HTMLElement) {
  const top = cards.scrollTop;
  const bounds = cards.getBoundingClientRect();
  const anchor = top > 0 ? [...cards.querySelectorAll<HTMLElement>(".card")].find(card =>
    card.getBoundingClientRect().bottom > bounds.top) : undefined;
  return { top, id: anchor?.dataset.id, offset: anchor ? anchor.getBoundingClientRect().top - bounds.top : 0 };
}

function restoreScroll(cards: HTMLElement, position?: ReturnType<typeof captureScroll>) {
  cards.scrollTop = position?.top ?? 0;
  if (!position?.id) return;
  const anchor = [...cards.querySelectorAll<HTMLElement>(".card")].find(card => card.dataset.id === position.id);
  if (anchor) cards.scrollTop += anchor.getBoundingClientRect().top - cards.getBoundingClientRect().top - position.offset;
}

function isDisabled(element: HTMLElement | undefined) {
  return element instanceof HTMLButtonElement && element.disabled;
}

export function createBoardLayout({ viewport, unplaced, jump, filters }: BoardElements) {
  const root = viewport.getRootNode() as ShadowRoot | Document;
  const unplacedCards = unplaced.querySelector<HTMLElement>(".cards")!;
  const unplacedSummary = unplaced.querySelector("summary")!;
  const columns = () => [...viewport.querySelectorAll<HTMLElement>(".column")];
  const cards = () => [...viewport.querySelectorAll<HTMLElement>(".card"), ...unplacedCards.querySelectorAll<HTMLElement>(".card")];
  const columnCards = (column: HTMLElement) => column.querySelector<HTMLElement>(".cards")!;

  function updateJumps(nextColumns: HTMLElement[]) {
    const buttons = [...jump.querySelectorAll<HTMLButtonElement>("button")];
    const next = nextColumns.map(column => {
      const button = buttons.find(item => item.dataset.state === column.dataset.state) ?? document.createElement("button");
      button.dataset.state = column.dataset.state;
      const label = `${column.getAttribute("aria-label")} ${column.querySelector(".column-count")!.textContent}`;
      if (button.textContent !== label) button.textContent = label;
      button.onclick = () => {
        const board = viewport.querySelector<HTMLElement>(".board")!;
        viewport.scrollTo({ left: column.offsetLeft - board.offsetLeft, behavior: "smooth" });
      };
      return button;
    });
    if (next.length !== buttons.length || next.some((button, index) => buttons[index] !== button)) {
      jump.replaceChildren(...next);
    }
  }

  function update(model: BoardModel, render: () => void, resetState?: string | null) {
    const scroll = new Map(columns().map(column =>
      [column.dataset.state, column.dataset.state === resetState ? undefined : captureScroll(columnCards(column))]));
    const unplacedScroll = captureScroll(unplacedCards);
    const active = root.activeElement instanceof HTMLElement ? root.activeElement : undefined;
    const focused = active?.closest<HTMLElement>(".card");
    const focus = active && focused && {
      id: focused.dataset.id, state: focused.closest<HTMLElement>(".column")?.dataset.state,
      action: active.dataset.action || (active.closest(".project-tag") ? "project" : "preview"),
    };
    const expanded = new Map(cards().map(card => [card.dataset.id, {
      project: card.querySelector<HTMLDetailsElement>(".project-tag")!.open,
      note: Boolean(card.querySelector(".note-details[open]")),
    }]));
    unplaced.hidden = !model.unplaced.length && model.moving?.from !== null;
    unplacedSummary.textContent = `未归列会话 ${model.unplaced.length + Number(model.moving?.from === null)}`;
    render();
    restoreScroll(unplacedCards, unplacedScroll);
    const nextColumns = columns();
    updateJumps(nextColumns);
    const nextCards = cards();
    for (const card of nextCards) {
      const previous = expanded.get(card.dataset.id);
      if (!previous) continue;
      card.querySelector<HTMLDetailsElement>(".project-tag")!.open = previous.project;
      const note = card.querySelector<HTMLDetailsElement>(".note-details");
      if (note) {
        note.open = previous.note;
        if (note.open) note.classList.add("expandable");
      }
    }
    if (focus && (active !== root.activeElement || isDisabled(active))) {
      const card = nextCards.find(item => item.dataset.id === focus.id);
      const selector = focus.action === "project" ? ".project-tag summary" : `[data-action="${focus.action}"]`;
      const target = card?.querySelector<HTMLElement>(selector);
      const previousColumn = nextColumns.find(column => column.dataset.state === focus.state);
      const fallback = previousColumn?.querySelector<HTMLElement>(".column-sort") ??
        (unplaced.hidden ? filters.querySelector<HTMLElement>("#host-filter")! : unplacedSummary);
      (target && !isDisabled(target) ? target : fallback).focus({ preventScroll: true });
    }
    for (const column of nextColumns) restoreScroll(columnCards(column), scroll.get(column.dataset.state));
  }

  return { update, unplacedCards };
}
