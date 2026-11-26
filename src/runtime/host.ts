/// <reference lib="dom" />

import { parseUiPayload, type SidebarItemRegistration } from "./registry.js";

export interface UiHost {
    stop(): void;
}

export interface UiHostOptions {
    fetchState(): Promise<unknown>;
    subscribe(listener: (payload: unknown) => void): void;
    proof(stage: string, extra?: Record<string, unknown>): void;
    mountTasks?(container: HTMLElement): (() => void) | undefined;
    mountPage?(
        id: string,
        panel: HTMLElement,
        close: () => void,
        showConversation: (visible: boolean, mode?: "draft") => void,
        openConversation: (
            navigate: () => Promise<unknown>,
            origin?: HTMLElement,
        ) => Promise<boolean>,
    ): (() => void) | undefined;
}

const RAIL = "nav[data-app-navigation-rail]";
const DESTINATION = "[data-sidebar-destination]";
const WORKSPACE = "[data-app-shell-workspace]";
const MAIN_SURFACE = "[data-app-shell-main-surface]";
const ACTIVE_MAIN = `${WORKSPACE} ${MAIN_SURFACE}:not(${MAIN_SURFACE} *, [data-app-shell-active-page="false"] *)`;
const MAIN_LAYOUT = "[data-app-shell-main-content-layout]";
const MAIN_PANE = '[data-app-shell-focus-area="main"]';
const RIGHT_PANE = '[data-app-shell-focus-area="right-panel"]';
const SUMMARY_TOGGLE = "[data-thread-summary-panel-toggle]";
const POPOVER_CONTENT = '[data-slot="popover-content"]';
const POPPER_WRAPPER = "[data-radix-popper-content-wrapper]";
const SUMMARY_CONTENT = '[data-pip-obstacle="thread-summary-panel-popover"]';
const SUMMARY_POPUP = `${POPOVER_CONTENT}:has(${SUMMARY_CONTENT})`;
const INLINE_SUMMARY = '[data-pip-obstacle="thread-summary-panel"]';
type SummaryPlacement = "popover" | "inline";
const SUMMARY_INSET = 8;
const SUMMARY_SPACE = 300 + SUMMARY_INSET * 2;
const FULL_WIDTH_PANE = '[data-app-shell-right-panel-full-width="true"]';
const CONVERSATION_CONTENT =
    ':is([data-thread-find-target="conversation"], [data-thread-find-composer]):not([data-app-shell-focus-area="right-panel"] *)';
const TITLEBAR = '[data-app-shell-titlebar]:not([data-app-shell-active-page="false"] *)';
const DRAWER_TITLEBAR = "data-codex-enhancer-conversation-titlebar";
const PAGE_SIDEBAR = "[data-app-shell-page-sidebar]";
const SIDEBAR_SCROLL = "[data-app-action-sidebar-scroll]";
const ITEM = "data-codex-enhancer-item";
const PAGE = "data-codex-enhancer-page";
const SCRIM = "data-codex-enhancer-conversation-scrim";
const LOADING = "data-codex-enhancer-conversation-loading";
const NAVIGATION = `${RAIL}, ${DESTINATION}`;

interface Mount {
    parent: HTMLElement;
    sample: Element | null;
    kind: "row" | "rail";
}

interface ConversationOverlay {
    style: HTMLStyleElement;
    scrim: HTMLButtonElement;
    titlebar: HTMLElement | null;
    background: HTMLElement | null;
    previousFocus: HTMLElement | null;
    centered: boolean;
    loading: HTMLElement | null;
}

interface HostState {
    options: UiHostOptions;
    items: SidebarItemRegistration[];
    autoOpen: boolean;
    autoOpened: boolean;
    activeId: string | null;
    panel: HTMLElement | null;
    drawer: ConversationOverlay | null;
    unmount: (() => void) | undefined;
    tasks?: { container: HTMLElement; unmount: (() => void) | undefined };
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

function isSidebarInWorkspace(rect: DOMRect, workspace: DOMRect): boolean {
    return !(
        rect.width < 8 ||
        rect.height < 40 ||
        rect.width > workspace.width * 0.7 ||
        rect.right <= workspace.left + 1 ||
        rect.left >= workspace.right - 1 ||
        rect.left > workspace.left + workspace.width * 0.75
    );
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
        if (isSidebarInWorkspace(rect, bounds)) left = Math.max(left, rect.right);
    }
    const width = bounds.right - left;
    if (width < 80 || bounds.height < 80) return null;
    return new DOMRect(left, bounds.top, width, bounds.height);
}

function drawerTitlebar(drawer: ConversationOverlay): number {
    const surface = document.querySelector(ACTIVE_MAIN);
    const frame = surface?.closest("[data-app-shell-frame]") ?? document;
    const header = [...frame.querySelectorAll(TITLEBAR)].find(visible);
    const titlebar = surface && isHtmlElement(header) && !surface.contains(header) ? header : null;
    if (drawer.titlebar !== titlebar) {
        drawer.titlebar?.removeAttribute(DRAWER_TITLEBAR);
        titlebar?.setAttribute(DRAWER_TITLEBAR, "");
        drawer.titlebar = titlebar;
    }
    return titlebar?.getBoundingClientRect().height ?? 0;
}

function conversationBounds(box: DOMRect, centered: boolean, extraWidth: number): DOMRect {
    const inset = Math.min(16, Math.floor(box.width / 12));
    const availableWidth = box.width - inset * 2;
    if (centered) {
        const width = Math.min(availableWidth, Math.min(box.width * 0.88, 960) + extraWidth);
        const height = box.height * 0.86;
        return new DOMRect((box.width - width) / 2, (box.height - height) / 2, width, height);
    }
    const width = Math.min(availableWidth, Math.max(400, Math.ceil(availableWidth * 0.87)));
    return new DOMRect(box.width - width - inset, 40, width, box.height - 52);
}

function rectangleStyle({ left, top, width, height }: DOMRect): Record<string, string> {
    return { left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px` };
}

function conversationTabs() {
    const surface = document.querySelector(ACTIVE_MAIN);
    const pane = surface && [...surface.querySelectorAll(RIGHT_PANE)].find(visible);
    if (isHtmlElement(pane)) {
        return {
            extraWidth: 420,
            splitWidth: surface?.querySelector(FULL_WIDTH_PANE) ? null : pane.style.width,
        };
    }
    return null;
}

function conversationSummary(titlebar: HTMLElement | null): SummaryPlacement | null {
    const trigger =
        titlebar?.querySelector(SUMMARY_TOGGLE) ??
        document.querySelector(`${ACTIVE_MAIN} ${SUMMARY_TOGGLE}`);
    const expanded = trigger?.getAttribute("aria-expanded");
    if (expanded === "true") return "popover";
    return expanded == null && trigger?.getAttribute("aria-pressed") === "true" ? "inline" : null;
}

function placeDrawer(box: DOMRect, panel: HTMLElement, drawer: ConversationOverlay): void {
    const headerHeight = drawer.loading ? 0 : drawerTitlebar(drawer);
    const showAccessories = drawer.centered && !drawer.loading;
    const tabs = showAccessories ? conversationTabs() : null;
    const summary = showAccessories ? conversationSummary(drawer.titlebar) : null;
    const extraWidth = (tabs?.extraWidth ?? 0) + (summary ? SUMMARY_SPACE : 0);
    const bounds = conversationBounds(box, drawer.centered, extraWidth);
    if (drawer.loading) {
        const viewportBounds = new DOMRect(
            box.x + bounds.x,
            box.y + bounds.y,
            bounds.width,
            bounds.height,
        );
        Object.assign(drawer.loading.style, rectangleStyle(viewportBounds));
        return;
    }
    const { left, top, bottom, right, width } = bounds;
    const radius = drawer.centered ? Math.min(16, bounds.height / 2, width / 2) : 10;
    const titlebarHeight = Math.min(headerHeight, Math.max(0, bounds.height - 1));
    const drawerLeft = box.x + left;
    const body = new DOMRect(
        drawerLeft,
        box.y + top + titlebarHeight,
        width,
        bounds.height - titlebarHeight,
    );
    const rules = `
        ${ACTIVE_MAIN}, [${DRAWER_TITLEBAR}] {
            position: fixed !important;
            margin: 0 !important;
            width: ${width}px !important;
            box-sizing: border-box;
            background: var(--color-surface, Canvas) !important;
        }
        ${ACTIVE_MAIN} {
            inset: ${body.top}px auto auto ${drawerLeft}px !important;
            height: ${body.height}px !important;
            border-radius: ${titlebarHeight ? 0 : radius}px ${titlebarHeight ? 0 : radius}px ${radius}px ${radius}px;
            overflow: hidden;
        }
        ${ACTIVE_MAIN} ${MAIN_LAYOUT} > :is(:only-child, :has(${MAIN_PANE})) {
            width: 100% !important;
        }
        ${showAccessories ? centeredConversationRules(body, tabs?.splitWidth ?? null, summary) : ""}
        [${DRAWER_TITLEBAR}] {
            inset: ${box.y + top}px auto auto ${drawerLeft}px !important;
            height: ${titlebarHeight}px !important;
            z-index: 31;
            border-radius: ${radius}px ${radius}px 0 0;
            -webkit-app-region: no-drag;
        }
        [${DRAWER_TITLEBAR}] [data-app-shell-header-slot="start"] {
            width: max-content !important;
        }
        [${SCRIM}]::before {
            content: "";
            position: absolute;
            inset: ${top}px ${box.width - right}px ${box.height - bottom}px ${left}px;
            border-radius: ${radius}px;
            box-shadow: 0 8px 30px light-dark(#252b3a48, #0009);
        }
    `;
    if (drawer.style.textContent !== rules) drawer.style.textContent = rules;
    const outline = `M0 0H${box.width}V${box.height}H0Z`;
    const opening =
        `M${left + radius} ${top}H${right - radius}q${radius} 0 ${radius} ${radius}` +
        `V${bottom - radius}q0 ${radius} -${radius} ${radius}` +
        `H${left + radius}q-${radius} 0 -${radius} -${radius}` +
        `V${top + radius}q0 -${radius} ${radius} -${radius}Z`;
    panel.style.clipPath = `path(evenodd, "${outline} ${opening}")`;
}

function centeredConversationRules(
    body: DOMRect,
    nativeTabWidth: string | null,
    summary: SummaryPlacement | null,
) {
    const columnWidth = Math.min(SUMMARY_SPACE, body.width / 3);
    const reservedWidth = summary ? columnWidth : 0;
    const sideLimit = `${(body.width - reservedWidth) * 0.46}px`;
    const tabWidth = nativeTabWidth ? `min(${nativeTabWidth}, ${sideLimit})` : null;
    return `
        ${nativeTabWidth != null ? centeredSplitRules(tabWidth, sideLimit) : ""}
        ${summary === "popover" ? summaryPopupRules(body, columnWidth, tabWidth ?? "0px") : ""}
        ${summary === "inline" ? inlineSummaryRules(columnWidth) : ""}
        ${POPPER_WRAPPER}:has(> ${POPOVER_CONTENT}[data-state="closed"] ${SUMMARY_CONTENT}) {
            visibility: hidden !important;
        }
    `;
}

function inlineSummaryRules(columnWidth: number): string {
    const layout = `${ACTIVE_MAIN} ${MAIN_LAYOUT}:not(${RIGHT_PANE} *)`;
    return `
        ${layout} [data-request-input-activity-root] {
            width: calc(100% - ${columnWidth}px) !important;
        }
        ${layout} :has(> ${INLINE_SUMMARY}) {
            max-width: ${columnWidth - SUMMARY_INSET}px !important;
        }
    `;
}

function summaryPopupRules(body: DOMRect, columnWidth: number, tabWidth: string): string {
    const width = columnWidth - SUMMARY_INSET * 2;
    return `
        ${ACTIVE_MAIN} ${MAIN_LAYOUT}:not(${RIGHT_PANE} *) {
            margin-inline-end: ${columnWidth}px !important;
        }
        ${POPPER_WRAPPER}:has(> ${POPOVER_CONTENT} ${SUMMARY_CONTENT}) {
            left: calc(${body.right - columnWidth + SUMMARY_INSET}px - ${tabWidth}) !important;
            top: ${body.top + SUMMARY_INSET}px !important;
            transform: none !important;
            --radix-popper-available-height: ${body.height - SUMMARY_INSET * 2}px !important;
        }
        ${SUMMARY_POPUP} {
            clip-path: none !important;
            width: ${width}px !important;
        }
        ${SUMMARY_POPUP} ${SUMMARY_CONTENT} {
            max-width: 100% !important;
        }
    `;
}

function centeredSplitRules(tabWidth: string | null, sideLimit: string): string {
    const titlebar = `[${DRAWER_TITLEBAR}]`;
    const outerPane = `${RIGHT_PANE}:not(${RIGHT_PANE} *)`;
    const headerPane = `${titlebar} ${outerPane}`;
    const bodyPane = `${ACTIVE_MAIN} ${outerPane}`;
    return `
        ${bodyPane},
        ${headerPane},
        ${titlebar} [data-app-shell-header-slot="end"] {
            min-width: 0 !important;
            max-width: ${sideLimit} !important;
        }
        ${headerPane} > :only-child,
        ${bodyPane} > div > div > div:has([data-app-shell-tab-panel-controller="right"]) {
            width: 100% !important;
        }
        ${tabWidth ? `${titlebar} [data-app-shell-main-titlebar] { inset-inline-end: ${tabWidth} !important; }` : ""}
    `;
}

function place(host: HostState): void {
    const element = host.panel;
    if (element == null) return;
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
    element.style.inset = box ? `${box.y}px auto auto ${box.x}px` : "48px";
    element.style.width = box ? `${box.width}px` : "auto";
    element.style.height = box ? `${box.height}px` : "auto";
    if (box && host.drawer) placeDrawer(box, element, host.drawer);
    else {
        if (host.drawer?.style.textContent) host.drawer.style.textContent = "";
        element.style.clipPath = "";
    }
}

function closeConversation(host: HostState, restoreFocus = false): void {
    const drawer = host.drawer;
    if (drawer == null) return;
    host.drawer = null;
    drawer.style.remove();
    drawer.titlebar?.removeAttribute(DRAWER_TITLEBAR);
    drawer.loading?.remove();
    drawer.scrim.remove();
    if (drawer.background) drawer.background.inert = false;
    place(host);
    if (restoreFocus) drawer.previousFocus?.focus({ preventScroll: true });
}

function mountConversation(
    host: HostState,
    panel: HTMLElement,
    loading: HTMLElement | null,
    returnFocus: Element | null,
) {
    if (host.drawer) return null;
    const content = panel.firstElementChild;
    const background = isHtmlElement(content) && !content.inert ? content : null;
    const scrim = document.createElement("button");
    scrim.type = "button";
    scrim.setAttribute(SCRIM, "");
    scrim.style.cssText =
        "position:absolute;inset:0;z-index:1;border:0;cursor:pointer;background:light-dark(#262d431b,#0b0c1070);color:inherit;font:inherit;-webkit-app-region:no-drag;";
    const label = document.createElement("span");
    label.textContent = "关闭会话";
    label.style.cssText =
        "position:absolute;top:6px;right:16px;padding:3px 10px;border:1px solid color-mix(in srgb,currentColor 18%,transparent);border-radius:8px;background:Canvas;";
    scrim.append(label);
    scrim.addEventListener("click", () => closeConversation(host, true));
    const style = document.createElement("style");
    const drawer: ConversationOverlay = {
        style,
        scrim,
        titlebar: null,
        background,
        previousFocus: isHtmlElement(returnFocus) ? returnFocus : null,
        centered: loading !== null,
        loading,
    };
    host.drawer = drawer;
    if (background) background.inert = true;
    document.head.append(style);
    panel.append(scrim);
    if (loading) panel.append(loading);
    place(host);
    return drawer;
}

function showConversation(host: HostState, visible: boolean, mode?: "draft"): void {
    if (!visible) {
        closeConversation(host);
        return;
    }
    const panel = host.panel;
    if (host.drawer || !panel) return;
    const surface = document.querySelector(ACTIVE_MAIN);
    if (!isHtmlElement(surface) || !contentBox())
        throw new Error("当前 Codex 窗口未提供会话区域。");
    const root = panel.firstElementChild?.shadowRoot;
    const trigger = mode === "draft" ? root?.getElementById("new-session") : root?.activeElement;
    mountConversation(host, panel, null, trigger ?? document.activeElement);
    const editor =
        mode === "draft"
            ? [
                  ...surface.querySelectorAll<HTMLElement>('.ProseMirror[contenteditable="true"]'),
              ].find((element) => element.getClientRects().length > 0)
            : undefined;
    (editor ?? surface).focus({ preventScroll: true });
}

function loadingConversation(origin?: HTMLElement): HTMLElement {
    const loading = document.createElement("section");
    loading.setAttribute(LOADING, "");
    loading.setAttribute("aria-label", origin?.getAttribute("aria-label") ?? "会话");
    loading.style.cssText =
        "position:fixed;z-index:2;margin:0;box-sizing:border-box;overflow:hidden;border:1px solid color-mix(in srgb,currentColor 14%,transparent);border-radius:16px;background:var(--color-surface,Canvas);box-shadow:0 18px 56px light-dark(#141b353b,#0009);";
    const title = document.createElement("strong");
    title.style.cssText =
        "display:block;padding:14px 18px 0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";
    title.textContent = origin?.getAttribute("aria-label") ?? "会话";
    const status = document.createElement("div");
    status.setAttribute("role", "status");
    status.style.cssText =
        "position:absolute;inset:0;display:grid;place-items:center;pointer-events:none;opacity:.72;";
    status.textContent = "正在打开会话…";
    loading.append(title, status);
    return loading;
}

async function animateConversation(drawer: ConversationOverlay, origin?: DOMRect): Promise<void> {
    const loading = drawer.loading;
    if (
        !loading ||
        !origin?.width ||
        !origin.height ||
        matchMedia("(prefers-reduced-motion: reduce)").matches
    )
        return;
    const motion = loading.animate(
        [{ ...rectangleStyle(origin), borderRadius: "10px", boxShadow: "none" }, {}],
        { duration: 360, easing: "cubic-bezier(.22,.78,.24,1)" },
    );
    drawer.scrim.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 280 });
    loading.lastElementChild?.animate(
        [{ opacity: 0 }, { opacity: 0, offset: 0.34 }, { opacity: 0.72 }],
        { duration: 350 },
    );
    await motion.finished;
}

async function waitForConversation(host: HostState, drawer: ConversationOverlay) {
    const deadline = performance.now() + 5000;
    while (host.drawer === drawer) {
        const surface = document.querySelector(ACTIVE_MAIN);
        if (
            isHtmlElement(surface) &&
            [...surface.querySelectorAll(CONVERSATION_CONTENT)].some(visible)
        )
            return surface;
        if (performance.now() >= deadline) throw new Error("等待 Codex 会话加载超时。");
        await new Promise<void>((resolve) => window.setTimeout(resolve, 50));
    }
    return null;
}

async function openConversation(
    host: HostState,
    navigate: () => Promise<unknown>,
    origin?: HTMLElement,
): Promise<boolean> {
    const panel = host.panel;
    if (!panel) return false;
    const box = contentBox();
    if (!box) throw new Error("当前 Codex 窗口未提供会话区域。");
    const start = origin?.getBoundingClientRect();
    const loading = loadingConversation(origin);
    const root = panel.firstElementChild?.shadowRoot;
    const focus = root?.activeElement ?? document.activeElement;
    const trigger = origin && !origin.contains(focus) ? origin.querySelector("button") : focus;
    const drawer = mountConversation(host, panel, loading, trigger);
    if (!drawer) return false;
    drawer.scrim.focus({ preventScroll: true });
    try {
        await Promise.all([animateConversation(drawer, start), navigate()]);
        const surface = await waitForConversation(host, drawer);
        if (!surface || host.drawer !== drawer) return false;
        loading.remove();
        drawer.loading = null;
        place(host);
        surface.focus({ preventScroll: true });
        return true;
    } catch (error) {
        if (host.drawer !== drawer) return false;
        closeConversation(host, true);
        throw error;
    }
}

function dismissOnEscape(host: HostState, event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.defaultPrevented || !host.drawer) return;
    if (isElement(event.target) && event.target.closest("dialog, [role=dialog], [role=menu]"))
        return;
    event.preventDefault();
    closeConversation(host, true);
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
        (document.body ?? document.documentElement).append(host.panel);
    }
    const panel = host.panel;
    panel.setAttribute(PAGE, item.id);
    host.unmount?.();
    closeConversation(host);
    panel.replaceChildren();
    host.unmount = host.options.mountPage?.(
        item.id,
        panel,
        () => closePage(host),
        (visible, mode) => {
            if (host.panel === panel) showConversation(host, visible, mode);
        },
        (navigate, origin) =>
            host.panel === panel
                ? openConversation(host, navigate, origin)
                : Promise.resolve(false),
    );
    if (!host.unmount) panel.append(pageContent(host, item));
    place(host);
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
    closeConversation(host);
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
        placeTasks(host);
        const mount = chooseMount();
        if (mount == null) {
            removeAllItems();
            reportMissing(host);
            place(host);
            return;
        }
        pruneItems(host, mount);
        for (const item of host.items) ensureItem(host, item, mount);
        reportMounted(host, mount);
        place(host);
    } finally {
        host.applying = false;
    }
}

function placeTasks(host: HostState): void {
    if (!host.options.mountTasks) return;
    const scroll = [...document.querySelectorAll(SIDEBAR_SCROLL)].find(visible);
    if (!scroll) return;
    if (!host.tasks) {
        const container = document.createElement("section");
        container.setAttribute("data-codex-enhancer-tasks", "");
        host.tasks = { container, unmount: host.options.mountTasks(container) };
    }
    if (host.tasks.container.parentElement !== scroll) scroll.prepend(host.tasks.container);
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
        attributeFilter: [
            "class",
            "style",
            "hidden",
            "inert",
            "aria-hidden",
            "aria-expanded",
            "aria-pressed",
            "data-app-shell-right-panel-full-width",
        ],
    });
    render(host);
}

function shellChanged(mutation: MutationRecord): boolean {
    if (mutation.type === "childList") return true;
    const element = mutation.target;
    return (
        isElement(element) &&
        (element.matches(`${NAVIGATION}, ${SUMMARY_TOGGLE}, ${RIGHT_PANE}, ${MAIN_LAYOUT}`) ||
            element.querySelector(NAVIGATION) != null)
    );
}

function stop(host: HostState, observer: MutationObserver): void {
    observer.disconnect();
    closePage(host);
    host.tasks?.unmount?.();
    host.tasks?.container.remove();
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
        drawer: null,
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
    const keydown = (event: KeyboardEvent): void => dismissOnEscape(host, event);
    window.addEventListener("keydown", keydown);
    const resize = (): void => render(host);
    const stopWatching = (): void => {
        window.removeEventListener("keydown", keydown);
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
