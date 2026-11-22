/// <reference lib="dom" />

import { parseUiPayload, type SidebarItemRegistration } from "./registry.js";

export interface UiHost {
  stop(): void;
}

export interface UiHostOptions {
  fetchState(): Promise<unknown>;
  subscribe(listener: (payload: unknown) => void): void;
  proof(stage: string, extra?: Record<string, unknown>): void;
}

const RAIL = "nav[data-app-navigation-rail]";
const DESTINATION = "[data-sidebar-destination]";
const WORKSPACE = "[data-app-shell-workspace]";
const PAGE_SIDEBAR = "[data-app-shell-page-sidebar]";
const SIDEBAR_SCROLL = "[data-app-action-sidebar-scroll]";
const ITEM = "data-codex-enhancer-item";
const PAGE = "data-codex-enhancer-page";

function isElement(value: EventTarget | Node | null | undefined): value is Element {
  return value != null && typeof (value as Element).getAttribute === "function";
}

function isHtmlElement(value: EventTarget | Node | null | undefined): value is HTMLElement {
  return isElement(value) && "style" in value;
}

interface Mount {
  parent: HTMLElement;
  sample: Element | null;
  kind: "row" | "rail";
}

export function startUiHost(options: UiHostOptions): UiHost {
  let items: SidebarItemRegistration[] = [];
  let autoOpen = false;
  let autoOpened = false;
  let activeId: string | null = null;
  let panel: HTMLElement | null = null;
  let applying = false;
  let sawMount = false;
  let reportedMissing = false;

  function proof(stage: string, extra: Record<string, unknown> = {}): void {
    options.proof(stage, extra);
  }

  function shellFacts(): Record<string, unknown> {
    return {
      rail: document.querySelectorAll(RAIL).length,
      destinations: document.querySelectorAll(DESTINATION).length,
      workspace: document.querySelectorAll(WORKSPACE).length,
      pageSidebar: document.querySelectorAll(PAGE_SIDEBAR).length,
      mounted: document.querySelectorAll(`[${ITEM}]`).length,
      activeId,
    };
  }

  function visibleLabel(element: Element): string {
    const copy = element.cloneNode(true);
    if (!isHtmlElement(copy)) return "";
    for (const hidden of copy.querySelectorAll(".sr-only, [aria-hidden='true']")) hidden.remove();
    return copy.textContent?.replace(/\s+/g, " ").trim() ?? "";
  }

  function chooseMount(): Mount | null {
    const labeled = [...document.querySelectorAll(DESTINATION)].find((element) => {
      return element.closest(RAIL) == null && element.parentElement != null && visibleLabel(element).length > 0;
    });
    if (labeled?.parentElement) return { parent: labeled.parentElement, sample: labeled, kind: "row" };
    const rail = document.querySelector(RAIL);
    if (isHtmlElement(rail)) {
      return { parent: rail, sample: rail.querySelector(DESTINATION), kind: "rail" };
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

  function ensureItem(item: SidebarItemRegistration, mount: Mount): void {
    const selector = `[${ITEM}="${CSS.escape(item.id)}"]`;
    const existing = mount.parent.querySelector(selector);
    if (existing != null && existing.tagName === "BUTTON") {
      existing.setAttribute("aria-current", activeId === item.id ? "page" : "false");
      return;
    }
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
      if (activeId === item.id) closePage();
      else openPage(item);
    });
    const anchor = [...mount.parent.querySelectorAll(DESTINATION)].at(-1);
    if (anchor != null) anchor.after(button);
    else mount.parent.append(button);
  }

  function opaqueColor(start: Element | null): string {
    let current = start;
    while (isHtmlElement(current)) {
      const color = getComputedStyle(current).backgroundColor;
      const match = /rgba?\(([^)]+)\)/.exec(color);
      if (match != null) {
        const parts = match[1].split(",").map((part) => Number(part.trim()));
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

  function openPage(item: SidebarItemRegistration): void {
    activeId = item.id;
    if (panel == null) {
      panel = document.createElement("section");
      panel.setAttribute(PAGE, item.id);
      (document.body ?? document.documentElement).append(panel);
    }
    panel.replaceChildren();
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
    back.addEventListener("click", () => closePage());
    const inner = document.createElement("div");
    inner.style.padding = "32px";
    inner.append(heading, body, back);
    panel.append(inner);
    place(panel);
    const box = contentBox();
    proof("page-rendered", {
      id: item.id,
      route: item.route,
      left: box?.x ?? null,
      top: box?.y ?? null,
      width: box?.width ?? null,
      height: box?.height ?? null,
      background: panel.style.background,
      ...shellFacts(),
    });
    for (const button of document.querySelectorAll(`[${ITEM}]`)) {
      if (isHtmlElement(button)) {
        button.setAttribute("aria-current", button.getAttribute(ITEM) === item.id ? "page" : "false");
      }
    }
  }

  function closePage(): void {
    const wasOpen = panel != null || activeId != null;
    activeId = null;
    panel?.remove();
    panel = null;
    for (const button of document.querySelectorAll(`[${ITEM}]`)) button.removeAttribute("aria-current");
    if (wasOpen) proof("page-closed", shellFacts());
  }

  function render(): void {
    if (applying) return;
    applying = true;
    try {
      const mount = chooseMount();
      if (mount == null) {
        for (const node of document.querySelectorAll(`[${ITEM}]`)) node.remove();
        if (sawMount) {
          sawMount = false;
          proof("sidebar-removed", shellFacts());
        } else if (!reportedMissing) {
          reportedMissing = true;
          proof("shell-observed", shellFacts());
        }
        return;
      }
      for (const node of document.querySelectorAll(`[${ITEM}]`)) {
        const id = node.getAttribute(ITEM);
        if (id == null || !items.some((item) => item.id === id) || !mount.parent.contains(node)) node.remove();
      }
      for (const item of items) ensureItem(item, mount);
      if (!sawMount && items.length > 0) {
        sawMount = true;
        proof("sidebar-mounted", {
          kind: mount.kind,
          sampleClass: mount.sample?.className ?? "",
          ...shellFacts(),
        });
        if (autoOpen && !autoOpened) {
          autoOpened = true;
          const item = items[0];
          if (item != null) window.setTimeout(() => openPage(item), 1200);
        }
      }
      if (panel != null) place(panel);
    } finally {
      applying = false;
    }
  }

  function apply(payload: unknown): void {
    const state = parseUiPayload(payload);
    items = state.items;
    autoOpen = state.autoOpen;
    render();
  }

  const observer = new MutationObserver(() => render());
  function watchShell(): void {
    const root = document.documentElement;
    if (!isHtmlElement(root)) return;
    observer.observe(root, { childList: true, subtree: true });
    render();
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", watchShell, { once: true });
  } else {
    watchShell();
  }
  document.addEventListener(
    "click",
    (event) => {
      if (activeId == null) return;
      const target = event.target;
      if (!isElement(target)) return;
      if (target.closest(`[${ITEM}]`) != null || target.closest(`[${PAGE}]`) != null) return;
      if (target.closest(DESTINATION) != null || target.closest(RAIL) != null) closePage();
    },
    true,
  );
  window.addEventListener("resize", () => {
    if (panel != null) place(panel);
  });

  function stop(): void {
    observer.disconnect();
    closePage();
    for (const node of document.querySelectorAll(`[${ITEM}]`)) node.remove();
  }

  window.addEventListener("pagehide", () => stop(), { once: true });
  options.subscribe((payload) => {
    try {
      apply(payload);
    } catch (error) {
      proof("ui-items-failed", { error: error instanceof Error ? error.message : String(error) });
    }
  });
  options.fetchState().then(apply, (error: unknown) => {
    proof("ui-items-failed", { error: error instanceof Error ? error.message : String(error) });
  });
  window.setTimeout(() => {
    if (!sawMount) proof("shell-observed", { delayed: true, ...shellFacts() });
  }, 4000);

  return { stop };
}
