import type { BoardSnapshot } from "./board-start";

interface RefreshPorts {
  request(path: "/api/sessions" | "/api/refresh", body?: object): Promise<unknown>;
  locked(): boolean;
  moving(): boolean;
  received(snapshot: BoardSnapshot): void;
  render(): void;
  renderHealth(): void;
  announce(message: string): void;
}

export function mountBoardRefresh(button: HTMLButtonElement, ports: RefreshPorts) {
  let generation = 0;
  let loaded = false;
  let error: string | null = null;

  const invalidate = () => ++generation;

  function reportError(cause: unknown) {
    error = cause instanceof Error ? cause.message : String(cause);
  }

  async function load() {
    if (ports.locked()) return;
    const requested = invalidate();
    try {
      const snapshot = await ports.request("/api/sessions") as BoardSnapshot;
      if (requested !== generation || ports.locked()) return;
      loaded = true;
      error = null;
      ports.received(snapshot);
    } catch (cause) {
      if (requested !== generation) return;
      reportError(cause);
      ports.render();
    }
  }

  button.onclick = async () => {
    try {
      await ports.request("/api/refresh", {});
      if (ports.moving()) ports.announce("已请求刷新；当前保存确认后会更新卡片位置。");
      await load();
    } catch (cause) {
      reportError(cause);
      ports.renderHealth();
    }
  };

  return {
    load, invalidate,
    get generation() { return generation; },
    get loaded() { return loaded; },
    get error() { return error; },
  };
}
