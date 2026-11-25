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

function feedbackText(
    operation: MoveOperation,
    label: string | undefined,
    waiting: boolean,
    slow: boolean,
) {
    if (waiting)
        return {
            heading: slow ? "还在等确认，龙娘陪你等" : "龙娘抱着卡片，等保存确认",
            detail: `${label} · 等待${operation.host}确认\n${operation.title}`,
        };
    if (operation.result === "saved")
        return {
            heading: "保存好了！",
            detail: label === undefined ? "状态已保存，当前未归列" : `已移到${label}`,
        };
    return {
        heading: "这次保存还需要核对",
        detail: "未收到保存确认，先显示上次已知位置。请核对后再操作。",
    };
}

export function mountMoveFeedback(elements: FeedbackElements, ports: FeedbackPorts) {
    let moving: MoveOperation | null = null;
    let activeFeedback: MoveOperation | null = null;
    let artReady = false;

    function dismiss() {
        const feedback = activeFeedback;
        if (feedback) {
            feedback.dismissed = true;
            for (const timer of feedback.timers) window.clearTimeout(timer);
            window.clearTimeout(feedback.hideTimer);
        }
        activeFeedback = null;
        elements.panel.hidden = true;
    }

    function artLoaded() {
        artReady = true;
        update();
    }

    function update(operation = activeFeedback) {
        if (
            !operation ||
            operation !== activeFeedback ||
            operation.dismissed ||
            document.hidden ||
            !artReady
        )
            return;
        const elapsed = performance.now() - operation.started;
        const waiting = operation.result === undefined;
        if (waiting && elapsed < delays.character) return;
        operation.shownAt ??= performance.now();
        show(operation, waiting, elapsed >= delays.slow);
        if (operation.result !== "saved") return;
        operation.visibleSince ??= performance.now();
        window.clearTimeout(operation.hideTimer);
        operation.hideTimer = window.setTimeout(
            () => {
                if (activeFeedback === operation && !document.hidden) dismiss();
            },
            Math.max(0, operation.remaining - (performance.now() - operation.visibleSince)),
        );
    }

    function show(operation: MoveOperation, waiting: boolean, slow: boolean) {
        const { panel, heading, detail } = elements;
        const states = ports.states();
        const label =
            operation.state !== null && Object.hasOwn(states, operation.state)
                ? states[operation.state]
                : undefined;
        const text = feedbackText(operation, label, waiting, slow);
        panel.hidden = false;
        panel.className = `move-feedback ${operation.result ?? "waiting"}`;
        heading.textContent = text.heading;
        detail.textContent = text.detail;
        detail.title = operation.title;
        if (waiting && slow && !operation.slow) {
            operation.slow = true;
            ports.announce("这次响应较慢，仍在等待保存确认。");
        }
    }

    function resetEditor() {
        const { editor, slot, panel } = elements;
        if (moving || !editor.contains(panel)) return;
        dismiss();
        slot.append(panel);
    }

    function start(session: BoardSession, state: string | null) {
        const { editor, form, slot, panel } = elements;
        dismiss();
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
        moving = operation;
        activeFeedback = operation;
        (editor.open ? form : slot).append(panel);
        panel.hidden = true;
        panel.className = "move-feedback waiting";
        operation.timers = [
            window.setTimeout(() => update(operation), delays.character),
            window.setTimeout(() => update(operation), delays.slow),
        ];
        ports.announce(
            `正在移到${state === null ? undefined : ports.states()[state]}，等待${host}确认。`,
        );
        ports.render();
        return operation;
    }

    function finish(operation: MoveOperation, saved: boolean) {
        for (const timer of operation.timers) window.clearTimeout(timer);
        if (moving === operation) moving = null;
        if (activeFeedback !== operation || operation.dismissed) return;
        if (
            operation.shownAt === undefined &&
            performance.now() - operation.started < delays.character
        ) {
            dismiss();
            return;
        }
        operation.result = saved ? "saved" : "unconfirmed";
        if (saved) elements.slot.append(elements.panel);
        update(operation);
    }

    function visibilityChanged() {
        if (!document.hidden) {
            update();
            return;
        }
        const operation = activeFeedback;
        if (operation?.visibleSince === undefined) return;
        operation.remaining = Math.max(
            0,
            operation.remaining - (performance.now() - operation.visibleSince),
        );
        delete operation.visibleSince;
        window.clearTimeout(operation.hideTimer);
    }

    function confirmUnload(event: BeforeUnloadEvent) {
        if (!moving) return;
        event.preventDefault();
        event.returnValue = "";
    }

    elements.dismiss.onclick = dismiss;
    elements.image.decode().then(artLoaded, () => {
        elements.image.hidden = true;
        artLoaded();
    });
    const signal = ports.signal;
    document.addEventListener("visibilitychange", visibilityChanged, { signal });
    window.addEventListener("beforeunload", confirmUnload, { signal });
    signal.addEventListener("abort", dismiss, { once: true });

    return {
        get moving() {
            return moving;
        },
        resetEditor,
        start,
        finish,
    };
}
