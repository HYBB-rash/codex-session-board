import { ref, shallowRef } from "vue";
import type { BoardDependencies } from "./columns";
import type { BoardSession } from "./session";

function scrollAtPoint(viewport: HTMLElement, point: { x: number; y: number }) {
    const bounds = viewport.getBoundingClientRect();
    if (!(point.y >= bounds.top && point.y <= bounds.bottom)) return;
    if (point.x > bounds.right - 45) viewport.scrollLeft += 12;
    else if (point.x < bounds.left + 45) viewport.scrollLeft -= 12;
    for (const list of viewport.querySelectorAll<HTMLElement>(".cards")) {
        const rect = list.getBoundingClientRect();
        if (point.x < rect.left || point.x > rect.right) continue;
        if (point.y < rect.top + 45) list.scrollTop -= 10;
        else if (point.y > rect.bottom - 45) list.scrollTop += 10;
    }
}

export function createCardDrag(viewport: HTMLElement, dependencies: BoardDependencies) {
    const dragged = shallowRef<BoardSession | null>(null);
    const destination = ref<string | null>(null);
    let dragPoint: { x: number; y: number } | null = null;
    let frame = 0;
    let suppressClickUntil = 0;

    function track(event: DragEvent) {
        if (dragged.value) dragPoint = { x: event.clientX, y: event.clientY };
    }

    function scroll() {
        if (!dragged.value) return;
        frame = requestAnimationFrame(scroll);
        if (dragPoint) scrollAtPoint(viewport, dragPoint);
    }

    function leaveBoard(event: DragEvent) {
        if (!(event.relatedTarget instanceof Node && viewport.contains(event.relatedTarget)))
            dragPoint = null;
    }

    function cancel() {
        if (!dragged.value) return;
        end();
        dependencies.refresh();
    }

    function end() {
        if (!dragged.value) return;
        suppressClickUntil = performance.now() + 400;
        dragged.value = null;
        destination.value = null;
        dragPoint = null;
        cancelAnimationFrame(frame);
    }

    function start(session: BoardSession, event: DragEvent) {
        if (
            !dependencies.actions(session).canAdjust ||
            dependencies.editorOpen() ||
            (event.target as Element).closest(".card-actions") ||
            !event.dataTransfer
        ) {
            event.preventDefault();
            return;
        }
        dragged.value = { ...session };
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", session.id);
        track(event);
        frame = requestAnimationFrame(scroll);
        dependencies.announce("拖到目标状态列后松开鼠标。");
    }

    function over(state: string, event: DragEvent) {
        if (!dragged.value || !event.dataTransfer) return;
        event.preventDefault();
        const accepts = dragged.value.state !== state || state === "blocked";
        destination.value = accepts ? state : null;
        event.dataTransfer.dropEffect = accepts ? "move" : "none";
    }

    function leave(state: string, event: DragEvent) {
        if (
            destination.value === state &&
            !(event.currentTarget as Element).contains(event.relatedTarget as Node | null)
        )
            destination.value = null;
    }

    function drop(state: string, event: DragEvent) {
        const session = dragged.value;
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
    }

    return {
        track,
        get source() {
            return dragged.value;
        },
        get target() {
            return destination.value;
        },
        get suppressesClick() {
            return !!dragged.value || performance.now() < suppressClickUntil;
        },
        start,
        over,
        leave,
        drop,
        leaveBoard,
        cancel,
    };
}
