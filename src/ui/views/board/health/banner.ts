import type { HealthReport } from "./report";

interface HealthElements {
    banner: HTMLElement;
    summary: HTMLElement;
    more: HTMLButtonElement;
    detail: HTMLDialogElement;
    list: HTMLElement;
    close: HTMLButtonElement;
    recheck: HTMLButtonElement;
    detailHelp: HTMLButtonElement;
    connectionHelp: HTMLButtonElement;
    recovery: HTMLDialogElement;
    recoveryClose: HTMLButtonElement;
    recoveryRecheck: HTMLButtonElement;
}

interface HealthPorts {
    reports(): HealthReport[];
    selectedHost(): string;
    readState(): { error: string | null; loaded: boolean };
    date(value: number): string;
    request(path: "/api/connect" | "/api/refresh", body: object): Promise<unknown>;
    load(): Promise<void>;
    announce(message: string, error?: boolean): void;
}

function element<Tag extends keyof HTMLElementTagNameMap>(
    tag: Tag,
    text?: string,
    className?: string,
) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
}

class BoardHealthView {
    private readonly connecting = new Set<string>();

    constructor(
        private readonly ui: HealthElements,
        private readonly ports: HealthPorts,
    ) {}

    private reportError(error: unknown) {
        this.ports.announce(error instanceof Error ? error.message : String(error), true);
    }

    private connectButton(key: string, hostId: string) {
        const button = element("button", "连接这台主机");
        button.type = "button";
        button.setAttribute("data-focus", `connect:${key}`);
        button.disabled = this.connecting.has(hostId);
        button.onclick = async () => {
            if (this.connecting.has(hostId)) return;
            this.connecting.add(hostId);
            button.disabled = true;
            try {
                await this.ports.request("/api/connect", { hostId });
                await this.ports.load();
            } catch (error) {
                this.reportError(error);
            } finally {
                this.connecting.delete(hostId);
                this.render();
            }
        };
        return button;
    }

    private diagnosticDetails(host: HealthReport, expanded: Set<string | null>) {
        const diagnostics = element("details", undefined, "diagnostics");
        const key = `diagnostics:${host.key}`;
        diagnostics.setAttribute("data-key", key);
        diagnostics.open = expanded.has(key);
        const title = element("summary", "技术诊断");
        title.setAttribute("data-focus", key);
        diagnostics.append(
            title,
            element("p", host.diagnostics.map(([label, error]) => `${label}：${error}`).join("\n")),
        );
        return diagnostics;
    }

    private hostDetails(
        host: HealthReport,
        expanded: Set<string | null>,
        firstExpanded: boolean,
        oldRecord: string,
    ) {
        const item = element("details", undefined, "health-host");
        item.setAttribute("data-key", host.key);
        item.open = expanded.has(host.key) || firstExpanded;
        const summary = element("summary", `${host.label}：${oldRecord}${host.status}`);
        summary.setAttribute("data-focus", host.key);
        const lastRead = host.lastRead
            ? `上次成功读取：${this.ports.date(host.lastRead)}`
            : "缓存时间未提供，不能确认资料是否最新。";
        const steps = host.canConnect
            ? "此主机未启用连接。点击下方连接按钮后，再查看读取结果。"
            : host.key === "service"
              ? "请重新检查。仍未恢复时，可把技术诊断提供给维护者。"
              : host.attention
                ? "请先确认这台主机的 Codex 是否可用，再点击“重新检查”。仍未恢复时，可把技术诊断提供给维护者。"
                : "需要更新结果时，可点击“重新检查”。";
        item.append(
            summary,
            element("p", host.effects.join("\n")),
            element("p", lastRead, "reason"),
            element("p", steps, "reason"),
        );
        if (host.canConnect && host.hostId) item.append(this.connectButton(host.key, host.hostId));
        if (host.diagnostics.length) item.append(this.diagnosticDetails(host, expanded));
        return item;
    }

    private renderDetails(reports = this.ports.reports()) {
        const { list } = this.ui;
        const selectedHost = this.ports.selectedHost();
        const first = !list.children.length;
        const expanded = new Set(
            [...list.querySelectorAll<HTMLDetailsElement>("details[open]")].map((node) =>
                node.getAttribute("data-key"),
            ),
        );
        const active =
            document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
        const focus = active?.getAttribute("data-focus");
        const focusedHost = active?.closest(".health-host")?.getAttribute("data-key");
        const oldRecord = this.ports.readState().error ? "上次记录 · " : "";
        const nodes = [...reports]
            .sort(
                (first, second) =>
                    Number(second.key === selectedHost) - Number(first.key === selectedHost),
            )
            .map((host) =>
                this.hostDetails(
                    host,
                    expanded,
                    first &&
                        (host.key === selectedHost || (selectedHost === "all" && host.attention)),
                    oldRecord,
                ),
            );
        list.replaceChildren(...nodes);
        const targets = [...list.querySelectorAll<HTMLElement>("[data-focus]")];
        if (focus)
            (
                targets.find((node) => node.getAttribute("data-focus") === focus) ||
                targets.find((node) => node.getAttribute("data-focus") === focusedHost)
            )?.focus({ preventScroll: true });
    }

    render() {
        const { ports, ui } = this;
        const selectedHost = ports.selectedHost();
        const reports = ports.reports(),
            current = reports.find((host) => host.key === selectedHost);
        const { error, loaded } = ports.readState();
        const others = reports.filter(
            (host) => host.key !== selectedHost && host.key !== "service" && host.attention,
        ).length;
        const attention = reports.some((host) => host.attention);
        const ready = current?.ready && !attention;
        const summary = error
            ? `看板连接暂时断开，${loaded ? "显示上次结果" : "尚未读取会话"}`
            : ready
              ? `${current.label} · 连接正常`
              : current
                ? `${current.label}：${current.status}`
                : others
                  ? `有 ${others} 台主机需要处理`
                  : "请选择主机查看读取与连接情况";
        const otherHosts = current && others ? ` · 另有 ${others} 台主机需要处理` : "";
        const readError = reports.some((host) => host.key === "service")
            ? " · 另有资料读取问题"
            : "";
        const message = `${summary}${otherHosts}${readError}`;
        if (ui.summary.textContent !== message) ui.summary.textContent = message;
        ui.banner.className = `health${attention ? " error" : ready ? " ready" : ""}`;
        if (ui.detail.open) this.renderDetails(reports);
    }

    private bindRecheck(button: HTMLButtonElement, after?: () => void) {
        button.onclick = async () => {
            button.disabled = true;
            try {
                await this.ports.request("/api/refresh", {});
                await this.ports.load();
                after?.();
            } catch (error) {
                this.reportError(error);
            } finally {
                button.disabled = false;
            }
        };
    }
    bind() {
        const { ui, ports } = this;
        const { detail, list, recovery } = ui;
        ui.more.onclick = () => {
            const selectedHost = ports.selectedHost();
            if (list.getAttribute("data-scope") !== selectedHost) list.replaceChildren();
            list.setAttribute("data-scope", selectedHost);
            this.renderDetails();
            detail.showModal();
        };
        ui.connectionHelp.onclick = ui.detailHelp.onclick = () => recovery.showModal();
        ui.close.onclick = () => detail.close();
        ui.recoveryClose.onclick = () => recovery.close();
        this.bindRecheck(ui.recheck);
        this.bindRecheck(ui.recoveryRecheck, () => {
            recovery.close();
            ports.announce("已请求重新检查连接，请查看上方连接状态。");
        });
    }
}

export function mountBoardHealth(ui: HealthElements, ports: HealthPorts) {
    const view = new BoardHealthView(ui, ports);
    view.bind();
    return { render: () => view.render() };
}
