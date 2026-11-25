import { ref, shallowRef } from "vue";
import type { BoardDependencies, BoardSession } from "./board-view";

export function createCardDrag(viewport: HTMLElement, dependencies: BoardDependencies) {
    return new CardDrag(viewport, dependencies);
}

class CardDrag {
    private readonly dragged = shallowRef<BoardSession | null>(null);
    private readonly destination = ref<string | null>(null);
    private point: { x: number; y: number } | null = null;
    private frame = 0;
    private suppressClickUntil = 0;

    constructor(
        private readonly viewport: HTMLElement,
        private readonly dependencies: BoardDependencies,
    ) {}

    readonly track = (event: DragEvent) => {
        if (this.source) this.point = { x: event.clientX, y: event.clientY };
    };

    private readonly scroll = () => {
        if (!this.source) return;
        this.frame = requestAnimationFrame(this.scroll);
        const { point, viewport } = this;
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
    };

    private end() {
        if (!this.source) return;
        this.suppressClickUntil = performance.now() + 400;
        this.dragged.value = null;
        this.destination.value = null;
        this.point = null;
        cancelAnimationFrame(this.frame);
    }

    get source() {
        return this.dragged.value;
    }

    get target() {
        return this.destination.value;
    }

    get suppressesClick() {
        return !!this.source || performance.now() < this.suppressClickUntil;
    }

    start(session: BoardSession, event: DragEvent) {
        const { dependencies } = this;
        if (
            !dependencies.actions(session).canAdjust ||
            dependencies.editorOpen() ||
            (event.target as Element).closest(".card-actions") ||
            !event.dataTransfer
        ) {
            event.preventDefault();
            return;
        }
        this.dragged.value = { ...session };
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", session.id);
        this.track(event);
        this.frame = requestAnimationFrame(this.scroll);
        dependencies.announce("拖到目标状态列后松开鼠标。");
    }

    over(state: string, event: DragEvent) {
        if (!this.source || !event.dataTransfer) return;
        event.preventDefault();
        const accepts = this.source.state !== state || state === "blocked";
        this.destination.value = accepts ? state : null;
        event.dataTransfer.dropEffect = accepts ? "move" : "none";
    }

    leave(state: string, event: DragEvent) {
        if (
            this.target === state &&
            !(event.currentTarget as Element).contains(event.relatedTarget as Node | null)
        )
            this.destination.value = null;
    }

    drop(state: string, event: DragEvent) {
        const { source: session, dependencies } = this;
        if (!session) return;
        event.preventDefault();
        this.end();
        if (!dependencies.actions(session).canAdjust) return;
        if (state === "blocked") {
            dependencies.edit(session.id, state, session);
            dependencies.announce("请说明卡在哪里，暂时不填也可以保存。");
        } else if (session.state === state) {
            dependencies.announce("状态未改变。");
            dependencies.refresh();
        } else dependencies.move(session, state);
    }

    readonly leaveBoard = (event: DragEvent) => {
        if (!(event.relatedTarget instanceof Node && this.viewport.contains(event.relatedTarget)))
            this.point = null;
    };

    readonly cancel = () => {
        if (!this.source) return;
        this.end();
        this.dependencies.refresh();
    };
}
