import type { BoardSnapshot } from "./board-start";

interface RefreshPorts {
  request(path: "/api/sessions" | "/api/refresh", body?: object): Promise<unknown>;
  locked(): boolean;
  moving(): boolean;
  currentScope(): string | undefined;
  received(snapshot: BoardSnapshot, accountChanged: boolean): void;
  render(): void;
  renderHealth(): void;
  announce(message: string): void;
}

export function mountBoardRefresh(button: HTMLButtonElement, ports: RefreshPorts, loaded = false) {
  let generation = 0;
  let error: string | null = null;
  let reading: { generation: number; promise: Promise<void> } | undefined;

  const invalidate = () => ++generation;

  function reportError(cause: unknown) {
    error = cause instanceof Error ? cause.message : String(cause);
  }

  async function read(requested: number) {
    try {
      const snapshot = await ports.request("/api/sessions") as BoardSnapshot;
      if (requested !== generation) return;
      const scope = ports.currentScope();
      const accountChanged = scope !== undefined && snapshot.accountScope !== undefined &&
        scope !== snapshot.accountScope;
      if (ports.locked() && !accountChanged) return;
      loaded = true;
      error = null;
      ports.received(snapshot, accountChanged);
    } catch (cause) {
      if (requested !== generation || ports.locked()) return;
      reportError(cause);
      ports.render();
    }
  }

  function load() {
    if (reading?.generation === generation) return reading.promise;
    const requested = invalidate();
    const promise = read(requested).finally(() => {
      if (reading?.generation === requested) reading = undefined;
    });
    reading = { generation: requested, promise };
    return promise;
  }

  button.onclick = async () => {
    try {
      await ports.request("/api/refresh", {});
      if (ports.moving()) ports.announce("已请求刷新；当前保存确认后会更新卡片位置。");
      invalidate();
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
