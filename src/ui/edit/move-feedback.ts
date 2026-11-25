import type { BoardSession } from "../session";

interface MoveOperation extends BoardSession {
    from: string | null;
    started: number;
    host: string;
    dismissed: boolean;
    timers: number[];
    result?: "saved" | "unconfirmed";
    shownAt?: number;
    slow?: boolean;
    remaining: number;
    visibleSince?: number;
    hideTimer?: number;
}

interface FeedbackElements {
    editor: HTMLDialogElement;
    form: HTMLFormElement;
    slot: HTMLElement;
    panel: HTMLElement;
    heading: HTMLElement;
    detail: HTMLElement;
    dismiss: HTMLButtonElement;
    image: HTMLImageElement;
}

const delays = { character: 1500, slow: 8000, success: 2000 };

interface FeedbackPorts {
    states(): Record<string, string>;
    announce(message: string): void;
    render(): void;
    signal: AbortSignal;
}

export function mountMoveFeedback(elements: FeedbackElements, ports: FeedbackPorts) {
    return new MoveFeedback(elements, ports);
}

class MoveFeedback {
    moving: MoveOperation | null = null;
    feedback: MoveOperation | null = null;
    artReady = false;

    constructor(
        private readonly elements: FeedbackElements,
        private readonly ports: FeedbackPorts,
    ) {
        elements.dismiss.onclick = this.dismiss;
        elements.image.decode().then(this.artLoaded, () => {
            elements.image.hidden = true;
            this.artLoaded();
        });
        const signal = ports.signal;
        document.addEventListener("visibilitychange", () => this.visibilityChanged(), { signal });
        window.addEventListener("beforeunload", (event) => this.confirmUnload(event), { signal });
        signal.addEventListener("abort", this.dismiss, { once: true });
    }

    private update(operation = this.feedback) {
        if (
            !operation ||
            operation !== this.feedback ||
            operation.dismissed ||
            document.hidden ||
            !this.artReady
        )
            return;
        const elapsed = performance.now() - operation.started;
        const waiting = operation.result === undefined;
        if (waiting && elapsed < delays.character) return;
        operation.shownAt ??= performance.now();
        this.show(operation, waiting, elapsed >= delays.slow);
        if (operation.result !== "saved") return;
        operation.visibleSince ??= performance.now();
        window.clearTimeout(operation.hideTimer);
        operation.hideTimer = window.setTimeout(
            () => {
                if (this.feedback === operation && !document.hidden) this.dismiss();
            },
            Math.max(0, operation.remaining - (performance.now() - operation.visibleSince)),
        );
    }

    private show(operation: MoveOperation, waiting: boolean, slow: boolean) {
        const { panel, heading, detail } = this.elements;
        const saved = operation.result === "saved";
        const states = this.ports.states();
        const label =
            operation.state !== null && Object.hasOwn(states, operation.state)
                ? states[operation.state]
                : undefined;
        panel.hidden = false;
        panel.className = `move-feedback ${operation.result ?? "waiting"}`;
        heading.textContent = waiting
            ? slow
                ? "还在等确认，龙娘陪你等"
                : "龙娘抱着卡片，等保存确认"
            : saved
              ? "保存好了！"
              : "这次保存还需要核对";
        detail.textContent = waiting
            ? `${label} · 等待${operation.host}确认\n${operation.title}`
            : saved
              ? label === undefined
                  ? "状态已保存，当前未归列"
                  : `已移到${label}`
              : "未收到保存确认，先显示上次已知位置。请核对后再操作。";
        detail.title = operation.title;
        if (waiting && slow && !operation.slow) {
            operation.slow = true;
            this.ports.announce("这次响应较慢，仍在等待保存确认。");
        }
    }

    private readonly dismiss = () => {
        const { feedback } = this;
        if (feedback) {
            feedback.dismissed = true;
            for (const timer of feedback.timers) window.clearTimeout(timer);
            window.clearTimeout(feedback.hideTimer);
        }
        this.feedback = null;
        this.elements.panel.hidden = true;
    };

    resetEditor() {
        const { editor, slot, panel } = this.elements;
        if (this.moving || !editor.contains(panel)) return;
        this.dismiss();
        slot.append(panel);
    }

    start(session: BoardSession, state: string | null) {
        const { ports } = this;
        const { editor, form, slot, panel } = this.elements;
        this.dismiss();
        const host = session.remote ? session.hostLabel || session.hostId || "远端" : "本机";
        const operation: MoveOperation = {
            ...session,
            state,
            host,
            from: session.state,
            started: performance.now(),
            dismissed: false,
            timers: [],
            remaining: delays.success,
        };
        this.moving = operation;
        this.feedback = operation;
        (editor.open ? form : slot).append(panel);
        panel.hidden = true;
        panel.className = "move-feedback waiting";
        operation.timers = [
            window.setTimeout(() => this.update(operation), delays.character),
            window.setTimeout(() => this.update(operation), delays.slow),
        ];
        ports.announce(
            `正在移到${state === null ? undefined : ports.states()[state]}，等待${host}确认。`,
        );
        ports.render();
        return operation;
    }

    finish(operation: MoveOperation, saved: boolean) {
        for (const timer of operation.timers) window.clearTimeout(timer);
        if (this.moving === operation) this.moving = null;
        if (this.feedback !== operation || operation.dismissed) return;
        if (
            operation.shownAt === undefined &&
            performance.now() - operation.started < delays.character
        ) {
            this.dismiss();
            return;
        }
        operation.result = saved ? "saved" : "unconfirmed";
        if (saved) this.elements.slot.append(this.elements.panel);
        this.update(operation);
    }

    private readonly artLoaded = () => {
        this.artReady = true;
        this.update();
    };

    private visibilityChanged() {
        if (!document.hidden) {
            this.update();
            return;
        }
        const operation = this.feedback;
        if (operation?.visibleSince === undefined) return;
        operation.remaining = Math.max(
            0,
            operation.remaining - (performance.now() - operation.visibleSince),
        );
        delete operation.visibleSince;
        window.clearTimeout(operation.hideTimer);
    }

    private confirmUnload(event: BeforeUnloadEvent) {
        if (!this.moving) return;
        event.preventDefault();
        event.returnValue = "";
    }
}
