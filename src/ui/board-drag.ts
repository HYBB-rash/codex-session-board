import { ref, shallowRef } from "vue";
import type { BoardDependencies, BoardSession } from "./board-view";

export function createCardDrag(viewport: HTMLElement, dependencies: BoardDependencies) {
  const source = shallowRef<BoardSession | null>(null);
  const target = ref<string | null>(null);
  let point: { x: number; y: number } | null = null;
  let frame = 0;
  let suppressClickUntil = 0;

  function track(event: DragEvent) {
    if (source.value) point = { x: event.clientX, y: event.clientY };
  }

  function scroll() {
    if (!source.value) return;
    frame = requestAnimationFrame(scroll);
    if (!point) return;
    const bounds = viewport.getBoundingClientRect();
    if (point.y >= bounds.top && point.y <= bounds.bottom) {
      if (point.x > bounds.right - 45) viewport.scrollLeft += 12;
      else if (point.x < bounds.left + 45) viewport.scrollLeft -= 12;
      for (const list of viewport.querySelectorAll<HTMLElement>(".cards")) {
        const rect = list.getBoundingClientRect();
        if (point.x < rect.left || point.x > rect.right) continue;
        if (point.y < rect.top + 45) list.scrollTop -= 10;
        else if (point.y > rect.bottom - 45) list.scrollTop += 10;
      }
    }
  }

  function end() {
    if (!source.value) return;
    suppressClickUntil = performance.now() + 400;
    source.value = null;
    target.value = null;
    point = null;
    cancelAnimationFrame(frame);
  }

  return {
    get source() { return source.value; },
    get target() { return target.value; },
    get suppressesClick() { return !!source.value || performance.now() < suppressClickUntil; },
    start(session: BoardSession, event: DragEvent) {
      if (!dependencies.actions(session).canAdjust || dependencies.editorOpen() ||
          (event.target as Element).closest(".card-actions") || !event.dataTransfer) {
        event.preventDefault();
        return;
      }
      source.value = { ...session };
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", session.id);
      track(event);
      frame = requestAnimationFrame(scroll);
      dependencies.announce("拖到目标状态列后松开鼠标。");
    },
    over(state: string, event: DragEvent) {
      if (!source.value || !event.dataTransfer) return;
      event.preventDefault();
      const accepts = source.value.state !== state || state === "blocked";
      target.value = accepts ? state : null;
      event.dataTransfer.dropEffect = accepts ? "move" : "none";
    },
    leave(state: string, event: DragEvent) {
      if (target.value === state && !(event.currentTarget as Element).contains(event.relatedTarget as Node | null))
        target.value = null;
    },
    drop(state: string, event: DragEvent) {
      const session = source.value;
      if (!session) return;
      event.preventDefault();
      end();
      if (!dependencies.actions(session).canAdjust) return;
      if (state === "blocked") {
        dependencies.edit(session.id, state, session);
        dependencies.announce("请说明卡在哪里，暂时不填也可以保存。");
      } else if (session.state === state) {
        dependencies.announce("状态未改变。");
        dependencies.refresh();
      } else dependencies.move(session, state);
    },
    track,
    leaveBoard(event: DragEvent) {
      if (!(event.relatedTarget instanceof Node && viewport.contains(event.relatedTarget))) point = null;
    },
    cancel() {
      if (!source.value) return;
      end();
      dependencies.refresh();
    },
  };
}
