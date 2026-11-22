/// <reference lib="dom" />

import { parseUiPayload, type SidebarItemRegistration } from "./registry.js";

export interface UiHost {
    stop(): void;
}

export interface UiHostOptions {
    fetchState(): Promise<unknown>;
    subscribe(listener: (payload: unknown) => void): void;
    proof(stage: string, extra?: Record<string, unknown>): void;
    mountPage?(id: string, panel: HTMLElement, close: () => void): (() => void) | undefined;
}

const RAIL = "nav[data-app-navigation-rail]";
const DESTINATION = "[data-sidebar-destination]";
const WORKSPACE = "[data-app-shell-workspace]";
const PAGE_SIDEBAR = "[data-app-shell-page-sidebar]";
const SIDEBAR_SCROLL = "[data-app-action-sidebar-scroll]";
const ITEM = "data-codex-enhancer-item";
const PAGE = "data-codex-enhancer-page";
const NAVIGATION = `${RAIL}, ${DESTINATION}`;

interface Mount {
    parent: HTMLElement;
    sample: Element | null;
    kind: "row" | "rail";
}

interface HostState {
    options: UiHostOptions;
    items: SidebarItemRegistration[];
    autoOpen: boolean;
    autoOpened: boolean;
    activeId: string | null;
    panel: HTMLElement | null;
    unmount: (() => void) | undefined;
    applying: boolean;
    sawMount: boolean;
    reportedMissing: boolean;
}

function isElement(value: EventTarget | Node | null | undefined): value is Element {
    return value != null && typeof (value as Element).getAttribute === "function";
}

function isHtmlElement(value: EventTarget | Node | null | undefined): value is HTMLElement {
    return isElement(value) && "style" in value;
}

function proof(host: HostState, stage: string, extra: Record<string, unknown> = {}): void {
    host.options.proof(stage, extra);
}

function shellFacts(host: HostState): Record<string, unknown> {
    return {
        rail: document.querySelectorAll(RAIL).length,
        destinations: document.querySelectorAll(DESTINATION).length,
        workspace: document.querySelectorAll(WORKSPACE).length,
        pageSidebar: document.querySelectorAll(PAGE_SIDEBAR).length,
        mounted: document.querySelectorAll(`[${ITEM}]`).length,
        activeId: host.activeId,
    };
}

function reportFailure(host: HostState, error: unknown): void {
    proof(host, "ui-items-failed", {
        error: error instanceof Error ? error.message : String(error),
    });
}

function visibleLabel(element: Element): string {
    const copy = element.cloneNode(true);
    if (!isHtmlElement(copy)) return "";
    for (const hidden of copy.querySelectorAll(".sr-only, [aria-hidden='true']")) hidden.remove();
    return copy.textContent?.replace(/\s+/g, " ").trim() ?? "";
}

function visible(element: Element): boolean {
    return (
        element.getClientRects().length > 0 &&
        getComputedStyle(element).visibility === "visible" &&
        element.closest("[inert], [aria-hidden='true']") == null
    );
}

function chooseMount(): Mount | null {
    const labeled = [...document.querySelectorAll(DESTINATION)].find((element) => {
        return (
            element.closest(RAIL) == null &&
            element.parentElement != null &&
            visible(element) &&
            visibleLabel(element).length > 0
        );
    });
    if (labeled?.parentElement)
        return { parent: labeled.parentElement, sample: labeled, kind: "row" };
    const rail = [...document.querySelectorAll(RAIL)].find(visible);
    if (isHtmlElement(rail)) {
        const sample = [...rail.querySelectorAll(DESTINATION)].find(visible) ?? null;
        return { parent: rail, sample, kind: "rail" };
    }
    return null;
}

function dashboardIcon(): SVGElement {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("width", "16");
    svg.setAttribute("height", "16");
    svg.setAttribute("aria-hidden", "true");
    for (const box of [
        { x: "1", y: "1", width: "6", height: "6" },
        { x: "9", y: "1", width: "6", height: "14" },
        { x: "1", y: "9", width: "6", height: "6" },
    ]) {
        const shape = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        shape.setAttribute("x", box.x);
        shape.setAttribute("y", box.y);
        shape.setAttribute("width", box.width);
        shape.setAttribute("height", box.height);
        shape.setAttribute("rx", "1");
        shape.setAttribute("fill", "none");
        shape.setAttribute("stroke", "currentColor");
        shape.setAttribute("stroke-width", "1.4");
        svg.append(shape);
    }
    return svg;
}

function itemButton(
    host: HostState,
    item: SidebarItemRegistration,
    mount: Mount,
): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = mount.sample?.className ?? "";
    button.setAttribute(ITEM, item.id);
    button.setAttribute("aria-label", item.title);
    button.title = item.title;
    const glyph = document.createElement("span");
    glyph.setAttribute("aria-hidden", "true");
    glyph.style.display = "inline-flex";
    glyph.append(dashboardIcon());
    const label = document.createElement("span");
    label.textContent = item.title;
    if (mount.kind === "rail") label.className = "sr-only";
    button.append(glyph, label);
    button.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (host.activeId === item.id) closePage(host);
        else openPage(host, item);
    });
    return button;
}

function ensureItem(host: HostState, item: SidebarItemRegistration, mount: Mount): void {
    const selector = `[${ITEM}="${CSS.escape(item.id)}"]`;
    const existing = mount.parent.querySelector(selector);
    if (existing != null && existing.tagName === "BUTTON") {
        existing.setAttribute("aria-current", host.activeId === item.id ? "page" : "false");
        return;
    }
    const button = itemButton(host, item, mount);
    const anchor = [...mount.parent.querySelectorAll(DESTINATION)].findLast(visible);
    if (anchor != null) anchor.after(button);
    else mount.parent.append(button);
}

function opaqueColor(start: Element | null): string {
    let current = start;
    while (isHtmlElement(current)) {
        const color = getComputedStyle(current).backgroundColor;
        const match = /rgba?\(([^)]+)\)/.exec(color);
        const captured = match?.[1];
        if (captured != null) {
            const parts = captured.split(",").map((part) => Number(part.trim()));
            const alpha = parts.length === 4 ? parts[3] : 1;
            if (alpha != null && alpha > 0.9) return color;
        }
        current = current.parentElement;
    }
    return "Canvas";
}

function contentBox(): DOMRect | null {
    const workspace = document.querySelector(WORKSPACE);
    if (!isHtmlElement(workspace)) return null;
    const bounds = workspace.getBoundingClientRect();
    let left = bounds.left;
    for (const selector of [RAIL, PAGE_SIDEBAR, SIDEBAR_SCROLL]) {
        const element = document.querySelector(selector);
        if (!isHtmlElement(element)) continue;
        const rect = element.getBoundingClientRect();
        if (rect.width < 8 || rect.height < 40) continue;
        if (rect.width > bounds.width * 0.7) continue;
        if (rect.right <= bounds.left + 1 || rect.left >= bounds.right - 1) continue;
        if (rect.left > bounds.left + bounds.width * 0.75) continue;
        left = Math.max(left, rect.right);
    }
    const width = bounds.right - left;
    if (width < 80 || bounds.height < 80) return null;
    return new DOMRect(left, bounds.top, width, bounds.height);
}

function place(element: HTMLElement): void {
    const box = contentBox();
    const workspace = document.querySelector(WORKSPACE);
    element.style.position = "fixed";
    element.style.zIndex = "30";
    element.style.boxSizing = "border-box";
    element.style.overflow = "auto";
    if (isHtmlElement(workspace)) {
        const style = getComputedStyle(workspace);
        element.style.background = opaqueColor(workspace);
        element.style.color = style.color;
        element.style.font = style.font;
    }
    if (box == null) {
        element.style.inset = "48px";
        return;
    }
    element.style.inset = "auto";
    element.style.left = `${box.x}px`;
    element.style.top = `${box.y}px`;
    element.style.width = `${box.width}px`;
    element.style.height = `${box.height}px`;
}

function nativeButtonClass(): string {
    const workspace = document.querySelector(WORKSPACE);
    if (!isHtmlElement(workspace)) return "";
    for (const button of workspace.querySelectorAll("button")) {
        if (button.hasAttribute(ITEM) || button.closest(`[${PAGE}]`) != null) continue;
        if ((button.textContent?.trim().length ?? 0) === 0) continue;
        return button.className;
    }
    return "";
}

function pageContent(host: HostState, item: SidebarItemRegistration): HTMLElement {
    const heading = document.createElement("h1");
    heading.textContent = item.page.title;
    heading.style.margin = "0 0 12px";
    heading.style.fontSize = "28px";
    const body = document.createElement("p");
    body.textContent = item.page.body;
    body.style.margin = "0";
    body.style.fontSize = "16px";
    const back = document.createElement("button");
    back.type = "button";
    back.textContent = "Back";
    back.className = nativeButtonClass();
    back.style.marginTop = "20px";
    back.addEventListener("click", () => closePage(host));
    const inner = document.createElement("div");
    inner.style.padding = "32px";
    inner.append(heading, body, back);
    return inner;
}

function markActiveItem(id: string): void {
    for (const button of document.querySelectorAll(`[${ITEM}]`)) {
        if (!isHtmlElement(button)) continue;
        button.setAttribute("aria-current", button.getAttribute(ITEM) === id ? "page" : "false");
    }
}

function openPage(host: HostState, item: SidebarItemRegistration): void {
    host.activeId = item.id;
    if (host.panel == null) {
        host.panel = document.createElement("section");
        host.panel.setAttribute(PAGE, item.id);
        (document.body ?? document.documentElement).append(host.panel);
    }
    host.unmount?.();
    host.panel.replaceChildren();
    host.unmount = host.options.mountPage?.(item.id, host.panel, () => closePage(host));
    if (!host.unmount) host.panel.append(pageContent(host, item));
    place(host.panel);
    const box = contentBox();
    proof(host, "page-rendered", {
        id: item.id,
        route: item.route,
        left: box?.x ?? null,
        top: box?.y ?? null,
        width: box?.width ?? null,
        height: box?.height ?? null,
        background: host.panel.style.background,
        ...shellFacts(host),
    });
    markActiveItem(item.id);
}

function closePage(host: HostState): void {
    const wasOpen = host.panel != null || host.activeId != null;
    host.activeId = null;
    host.unmount?.();
    host.unmount = undefined;
    host.panel?.remove();
    host.panel = null;
    for (const button of document.querySelectorAll(`[${ITEM}]`))
        button.removeAttribute("aria-current");
    if (wasOpen) proof(host, "page-closed", shellFacts(host));
}

function handleClick(host: HostState, event: MouseEvent): void {
    if (host.activeId == null) return;
    const target = event.target;
    if (!isElement(target)) return;
    if (target.closest(`[${ITEM}]`) != null || target.closest(`[${PAGE}]`) != null) return;
    if (target.closest(DESTINATION) != null || target.closest(RAIL) != null) closePage(host);
}

function removeAllItems(): void {
    for (const node of document.querySelectorAll(`[${ITEM}]`)) node.remove();
}

function reportMissing(host: HostState): void {
    if (host.sawMount) {
        host.sawMount = false;
        proof(host, "sidebar-removed", shellFacts(host));
        return;
    }
    if (host.reportedMissing) return;
    host.reportedMissing = true;
    proof(host, "shell-observed", shellFacts(host));
}

function pruneItems(host: HostState, mount: Mount): void {
    for (const node of document.querySelectorAll(`[${ITEM}]`)) {
        const id = node.getAttribute(ITEM);
        if (id == null || !host.items.some((item) => item.id === id)) {
            node.remove();
            continue;
        }
        if (!mount.parent.contains(node)) node.remove();
    }
}

function reportMounted(host: HostState, mount: Mount): void {
    if (host.sawMount || host.items.length === 0) return;
    host.sawMount = true;
    proof(host, "sidebar-mounted", {
        kind: mount.kind,
        sampleClass: mount.sample?.className ?? "",
        ...shellFacts(host),
    });
    if (!host.autoOpen || host.autoOpened) return;
    host.autoOpened = true;
    const item = host.items[0];
    if (item != null) window.setTimeout(() => openPage(host, item), 1200);
}

function render(host: HostState): void {
    if (host.applying) return;
    host.applying = true;
    try {
        const mount = chooseMount();
        if (mount == null) {
            removeAllItems();
            reportMissing(host);
            if (host.panel != null) place(host.panel);
            return;
        }
        pruneItems(host, mount);
        for (const item of host.items) ensureItem(host, item, mount);
        reportMounted(host, mount);
        if (host.panel != null) place(host.panel);
    } finally {
        host.applying = false;
    }
}

function apply(host: HostState, payload: unknown): void {
    const state = parseUiPayload(payload);
    host.items = state.items;
    host.autoOpen = state.autoOpen;
    render(host);
}

function watchShell(host: HostState, observer: MutationObserver): void {
    const root = document.documentElement;
    if (!isHtmlElement(root)) return;
    observer.observe(root, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ["class", "style", "hidden", "inert", "aria-hidden"],
    });
    render(host);
}

function shellChanged(mutation: MutationRecord): boolean {
    if (mutation.type === "childList") return true;
    const element = mutation.target;
    return (
        isElement(element) &&
        (element.matches(NAVIGATION) || element.querySelector(NAVIGATION) != null)
    );
}

function stop(host: HostState, observer: MutationObserver): void {
    observer.disconnect();
    closePage(host);
    for (const node of document.querySelectorAll(`[${ITEM}]`)) node.remove();
}

export function startUiHost(options: UiHostOptions): UiHost {
    const host: HostState = {
        options,
        items: [],
        autoOpen: false,
        autoOpened: false,
        activeId: null,
        unmount: undefined,
        panel: null,
        applying: false,
        sawMount: false,
        reportedMissing: false,
    };
    const observer = new MutationObserver((mutations) => {
        if (mutations.some(shellChanged)) render(host);
    });
    const watch = (): void => watchShell(host, observer);
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", watch, { once: true });
    } else {
        watch();
    }
    document.addEventListener("click", (event) => handleClick(host, event), true);
    const resize = (): void => render(host);
    const stopWatching = (): void => {
        window.removeEventListener("resize", resize);
        stop(host, observer);
    };
    window.addEventListener("resize", resize);
    window.addEventListener("pagehide", stopWatching, { once: true });
    options.subscribe((payload) => {
        try {
            apply(host, payload);
        } catch (error) {
            reportFailure(host, error);
        }
    });
    options.fetchState().then(
        (payload) => apply(host, payload),
        (error: unknown) => reportFailure(host, error),
    );
    window.setTimeout(() => {
        if (!host.sawMount) proof(host, "shell-observed", { delayed: true, ...shellFacts(host) });
    }, 4000);
    return { stop: stopWatching };
}
