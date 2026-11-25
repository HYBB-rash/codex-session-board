import type { BoardSnapshot } from "./controller";

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
    const refresh = new BoardRefresh(ports, loaded);
    button.onclick = () => refresh.requestRefresh();
    return refresh;
}

class BoardRefresh {
    generation = 0;
    error: string | null = null;
    private reading: { generation: number; promise: Promise<void> } | undefined;

    constructor(
        private readonly ports: RefreshPorts,
        public loaded: boolean,
    ) {}

    invalidate() {
        return ++this.generation;
    }

    private reportError(cause: unknown) {
        this.error = cause instanceof Error ? cause.message : String(cause);
    }

    private async read(requested: number) {
        const { ports } = this;
        try {
            const snapshot = (await ports.request("/api/sessions")) as BoardSnapshot;
            if (requested !== this.generation) return;
            const scope = ports.currentScope();
            const accountChanged =
                scope !== undefined &&
                snapshot.accountScope !== undefined &&
                scope !== snapshot.accountScope;
            if (ports.locked() && !accountChanged) return;
            this.loaded = true;
            this.error = null;
            ports.received(snapshot, accountChanged);
        } catch (cause) {
            if (requested !== this.generation || ports.locked()) return;
            this.reportError(cause);
            ports.render();
        }
    }

    load() {
        if (this.reading?.generation === this.generation) return this.reading.promise;
        const requested = this.invalidate();
        const promise = this.read(requested).finally(() => {
            if (this.reading?.generation === requested) this.reading = undefined;
        });
        this.reading = { generation: requested, promise };
        return promise;
    }

    async requestRefresh() {
        const { ports } = this;
        try {
            await ports.request("/api/refresh", {});
            if (ports.moving()) ports.announce("已请求刷新；当前保存确认后会更新卡片位置。");
            this.invalidate();
            await this.load();
        } catch (cause) {
            this.reportError(cause);
            ports.renderHealth();
        }
    }
}
