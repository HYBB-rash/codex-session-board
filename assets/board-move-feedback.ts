import type { BoardSession } from "./board-view";

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

export function mountMoveFeedback(elements: FeedbackElements, ports: {
  states(): Record<string, string>;
  announce(message: string): void;
  render(): void;
}) {
  let moving: MoveOperation | null = null;
  let feedback: MoveOperation | null = null;
  let artReady = false;
  const { editor, form, slot, panel, heading, detail, image } = elements;

  function update(operation = feedback) {
    if (!operation || operation !== feedback || operation.dismissed || document.hidden || !artReady) return;
    const elapsed = performance.now() - operation.started;
    const waiting = operation.result === undefined;
    if (waiting && elapsed < delays.character) return;
    operation.shownAt ??= performance.now();
    const saved = operation.result === "saved";
    const slow = elapsed >= delays.slow;
    const states = ports.states();
    const label = operation.state !== null && Object.hasOwn(states, operation.state) ? states[operation.state] : undefined;
    panel.hidden = false;
    panel.className = "move-feedback " + (operation.result ?? "waiting");
    heading.textContent = waiting
      ? slow ? "还在等确认，龙娘陪你等" : "龙娘抱着卡片，等保存确认"
      : saved ? "保存好了！" : "这次保存还需要核对";
    detail.textContent = waiting ? `${label} · 等待${operation.host}确认\n${operation.title}` : saved
      ? label === undefined ? "状态已保存，当前未归列" : `已移到${label}`
      : "未收到保存确认，先显示上次已知位置。请核对后再操作。";
    detail.title = operation.title;
    if (waiting && slow && !operation.slow) {
      operation.slow = true;
      ports.announce("这次响应较慢，仍在等待保存确认。");
    }
    if (!saved) return;
    operation.visibleSince ??= performance.now();
    window.clearTimeout(operation.hideTimer);
    operation.hideTimer = window.setTimeout(() => {
      if (feedback === operation && !document.hidden) dismiss();
    }, Math.max(0, operation.remaining - (performance.now() - operation.visibleSince)));
  }

  function dismiss() {
    if (feedback) {
      feedback.dismissed = true;
      feedback.timers.forEach(timer => window.clearTimeout(timer));
      window.clearTimeout(feedback.hideTimer);
    }
    feedback = null;
    panel.hidden = true;
  }

  function resetEditor() {
    if (moving || !editor.contains(panel)) return;
    dismiss();
    slot.append(panel);
  }

  function start(session: BoardSession, state: string | null) {
    dismiss();
    const host = session.remote ? session.hostLabel || session.hostId || "远端" : "本机";
    const operation: MoveOperation = {
      ...session, state, host, from: session.state, started: performance.now(),
      dismissed: false, timers: [], remaining: delays.success,
    };
    moving = operation;
    feedback = operation;
    (editor.open ? form : slot).append(panel);
    panel.hidden = true;
    panel.className = "move-feedback waiting";
    operation.timers = [
      window.setTimeout(() => update(operation), delays.character),
      window.setTimeout(() => update(operation), delays.slow),
    ];
    ports.announce(`正在移到${state === null ? undefined : ports.states()[state]}，等待${host}确认。`);
    ports.render();
    return operation;
  }

  function finish(operation: MoveOperation, saved: boolean) {
    operation.timers.forEach(timer => window.clearTimeout(timer));
    if (moving === operation) moving = null;
    if (feedback !== operation || operation.dismissed) return;
    if (operation.shownAt === undefined && performance.now() - operation.started < delays.character) {
      dismiss();
      return;
    }
    operation.result = saved ? "saved" : "unconfirmed";
    if (saved) slot.append(panel);
    update(operation);
  }

  elements.dismiss.onclick = dismiss;
  function artLoaded() {
    artReady = true;
    update();
  }
  image.decode().then(artLoaded, () => {
    image.hidden = true;
    artLoaded();
  });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      update();
      return;
    }
    const operation = feedback;
    if (operation?.visibleSince === undefined) return;
    operation.remaining = Math.max(0, operation.remaining - (performance.now() - operation.visibleSince));
    delete operation.visibleSince;
    window.clearTimeout(operation.hideTimer);
  });
  window.addEventListener("beforeunload", event => {
    if (!moving) return;
    event.preventDefault();
    event.returnValue = "";
  });
  return {
    start, finish, resetEditor,
    get moving() { return moving; },
    get feedback() { return feedback; },
    get artReady() { return artReady; },
  };
}
