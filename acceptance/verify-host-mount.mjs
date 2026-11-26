import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

const runtime = resolve(process.argv[2] ?? "dist/runtime");
const browser = process.env.BOARD_TEST_BROWSER ?? "google-chrome";
const fixture = `<!doctype html>
<title>Enhancer mount fixture</title>
<div id="row"><button data-sidebar-destination>Conversations</button></div>
<nav id="rail" data-app-navigation-rail>
  <button data-sidebar-destination>Apps</button>
  <div hidden><button data-sidebar-destination>Hidden rail group</button></div>
</nav>
<div data-app-shell-frame style="isolation:isolate">
  <div style="height:36px">
    <header data-app-shell-titlebar style="position:relative;display:flex;height:36px">
      <div data-app-shell-main-titlebar style="position:absolute;inset:0;display:flex">
        <button id="toggle-side-chat">Side chat</button>
        <button data-thread-summary-panel-toggle aria-expanded="false" aria-controls="summary-popup">Summary</button>
      </div>
      <div data-app-shell-header-slot="end" style="margin-left:auto;min-width:max-content"></div>
      <div id="tab-header" data-app-shell-focus-area="right-panel" style="position:absolute;inset-inline-end:0" hidden>
        <div id="tab-header-inner">
          <div data-app-shell-tab-strip-controller="right">
            <button id="preview-tab">Existing Codex tab</button>
          </div>
        </div>
      </div>
    </header>
  </div>
  <div data-app-shell-workspace style="position:relative;isolation:isolate;display:flex;height:600px;width:1464px;overflow:hidden">
    <main data-app-shell-main-surface tabindex="-1" style="flex:1;min-width:0">
    <div style="display:flex">
      <div data-app-shell-main-content-layout="thread-edge-scroll" style="display:flex;flex-direction:column;flex:1;min-width:0;overflow:hidden">
        <div id="conversation" data-app-action-timeline-scroll style="align-self:end;min-width:100%">
          <input aria-label="Main chat message"><button>Send</button>
        </div>
      </div>
      <aside data-app-shell-focus-area="right-panel" style="flex-shrink:0;height:240px" hidden>
        <div><div><div id="side-inner">
          <div data-app-shell-tab-panel-controller="right" data-app-action-timeline-scroll>
            <input aria-label="Side chat message"><button id="side-send">Send side chat</button>
          </div>
        </div></div></div>
      </aside>
    </div>
    </main>
  </div>
</div>
<div data-radix-popper-content-wrapper hidden style="position:fixed;left:0;top:0;z-index:50">
  <section id="summary-popup" data-slot="popover-content" data-state="closed" role="dialog"
    style="width:300px;background:white"><div style="width:300px"
      data-pip-obstacle="thread-summary-panel-popover">Summary contents</div></section>
</div>
<output id="result">waiting</output>
<script type="module">
  const row = document.querySelector("#row");
  const rail = document.querySelector("#rail");
  const result = document.querySelector("#result");
  const item = '[data-codex-enhancer-item="session-board"]';
  const page = '[data-codex-enhancer-page="session-board"]';
  const scrim = "[data-codex-enhancer-conversation-scrim]";
  const loading = "[data-codex-enhancer-conversation-loading]";
  const settle = () => new Promise(resolve => setTimeout(resolve, 30));
  async function check(parent, label) {
    await settle();
    const buttons = [...document.querySelectorAll(item)];
    const ok = parent
      ? buttons.length === 1 && parent.contains(buttons[0]) && buttons[0].getClientRects().length
      : buttons.length === 0;
    if (!ok) throw new Error(label);
  }
  try {
    const [registrySource, hostSource] = await Promise.all(
      ["/registry.js", "/host.js"].map(async path => (await fetch(path)).text()),
    );
    const registry = {};
    const runtime = {};
    new Function("exports", registrySource)(registry);
    new Function("exports", "require", hostSource)(runtime, name => {
      if (name === "./registry.js") return registry;
      throw new Error("unexpected module " + name);
    });
    row.hidden = true;
    let setConversationVisible;
    let openConversation;
    let unmounts = 0;
    const host = runtime.startUiHost({
      async fetchState() {
        return {items: [{id: "session-board", title: "Session Board", route: "/session-board",
          page: {title: "Session Board", body: "Fixture"}}], autoOpen: false};
      },
      subscribe() {},
      proof() {},
      mountPage(_id, panel, _close, showConversation, open) {
        setConversationVisible = showConversation;
        openConversation = open;
        const content = document.createElement("div");
        const card = document.createElement("button");
        card.textContent = "Example session";
        card.setAttribute("aria-label", card.textContent);
        content.append(document.createElement("input"), card);
        panel.append(content);
        return () => unmounts++;
      },
    });
    await check(rail, "initial hidden row did not fall back to the visible rail");
    document.querySelector(item).click();
    const openPage = document.querySelector(page);
    if (!openPage) throw new Error("rail launcher did not open the page");
    const workspace = document.querySelector("[data-app-shell-workspace]");
    const conversation = document.querySelector("[data-app-shell-main-surface]");
    const content = document.querySelector("#conversation");
    const sidePanel = document.querySelector("aside");
    const rightTabs = document.querySelector("#tab-header");
    const rightTabsInner = document.querySelector("#tab-header-inner");
    const sideInner = document.querySelector("#side-inner");
    const titlebar = document.querySelector("[data-app-shell-titlebar]");
    const mainHeader = titlebar.querySelector("[data-app-shell-main-titlebar]");
    const summary = titlebar.querySelector("[data-thread-summary-panel-toggle]");
    const rightHeader = titlebar.querySelector('[data-app-shell-header-slot="end"]');
    const toggleSideChat = titlebar.querySelector("button");
    const previewTab = rightTabs.querySelector("button");
    let sideRatio = 0.4;
    function sizeNativePanes() {
      const workspaceWidth = workspace.clientWidth;
      const sideWidth = Math.round(workspaceWidth * sideRatio);
      sidePanel.style.width = sideWidth + "px";
      sideInner.style.width = sideWidth + "px";
      rightTabs.style.width = sideWidth + "px";
      rightTabsInner.style.width = sideWidth + "px";
      rightHeader.style.width = sidePanel.hidden ? "auto" : sideWidth + "px";
      mainHeader.style.insetInlineEnd = sidePanel.hidden ? "0px" : sideWidth + "px";
      content.style.width = sidePanel.hidden ? "100%" : (workspaceWidth - sideWidth) + "px";
    }
    toggleSideChat.onclick = () => {
      sidePanel.hidden = !sidePanel.hidden;
      rightTabs.hidden = sidePanel.hidden;
      sideRatio = 0.4;
      sizeNativePanes();
    };
    previewTab.onclick = () => {
      sideRatio = 0.48;
      sizeNativePanes();
    };
    window.addEventListener("resize", sizeNativePanes);
    const input = openPage.querySelector("input");
    input.value = "keep the board state";
    input.focus();
    const fullWidth = openPage.getBoundingClientRect().width;
    setConversationVisible(true);
    const boardRect = openPage.getBoundingClientRect();
    const drawerRect = conversation.getBoundingClientRect();
    const titlebarRect = titlebar.getBoundingClientRect();
    if (boardRect.width !== fullWidth || drawerRect.left <= boardRect.left ||
        drawerRect.right >= boardRect.right || drawerRect.bottom >= boardRect.bottom ||
        drawerRect.top <= boardRect.top)
      throw new Error("native conversation does not float over the full-width board " +
        JSON.stringify({ fullWidth, boardRect, drawerRect }));
    if (drawerRect.left - boardRect.left > 203)
      throw new Error("the drawer is narrower than the marked edge in a 1464px workspace");
    if (titlebarRect.left !== drawerRect.left || titlebarRect.right !== drawerRect.right ||
        titlebarRect.top <= boardRect.top || titlebarRect.bottom !== drawerRect.top)
      throw new Error("the native titlebar and conversation do not share the same drawer");
    const backdrop = openPage.querySelector(scrim);
    if (!backdrop || document.elementFromPoint(boardRect.left + 8, drawerRect.top + 4) !== backdrop ||
        !input.parentElement.inert || document.activeElement !== conversation)
      throw new Error("the board is still interactive behind the conversation");
    function isClickable(control) {
      const bounds = control.getBoundingClientRect();
      return document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2) === control;
    }
    const near = (first, second) => Math.abs(first - second) < 1;
    if (!isClickable(toggleSideChat)) {
      const bounds = toggleSideChat.getBoundingClientRect();
      const hit = document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
      throw new Error("native titlebar controls are covered by " + hit?.outerHTML.slice(0, 170));
    }
    function checkNativeSplit(label, summaryGap = 0) {
      const surface = conversation.getBoundingClientRect();
      const mainBounds = content.getBoundingClientRect();
      const sideBounds = sidePanel.getBoundingClientRect();
      if (!near(mainBounds.left, surface.left) || !near(mainBounds.right + summaryGap, sideBounds.left))
        throw new Error(label + " clips the main chat " + JSON.stringify({surface, mainBounds, sideBounds}));
      if (sideBounds.right !== surface.right || sideBounds.width === 0 || !isClickable(previewTab) ||
          !isClickable(content.querySelector("input")) || !isClickable(sidePanel.querySelector("button")))
        throw new Error(label + " blocks the native chat controls");
    }
    toggleSideChat.click();
    checkNativeSplit("opening side chat");
    sidePanel.querySelector("input").value = "side chat draft";
    previewTab.click();
    checkNativeSplit("switching to a native tab");
    toggleSideChat.click();
    if (content.getBoundingClientRect().width !== drawerRect.width)
      throw new Error("hiding native tabs did not return the full width to the main chat");
    toggleSideChat.click();
    const originalWorkspaceWidth = workspace.style.width;
    workspace.style.width = "360px";
    window.dispatchEvent(new Event("resize"));
    const narrowDrawer = conversation.getBoundingClientRect();
    const narrowBoard = openPage.getBoundingClientRect();
    if (narrowDrawer.left <= narrowBoard.left || narrowDrawer.right >= narrowBoard.right ||
        narrowDrawer.top !== drawerRect.top)
      throw new Error("the drawer does not remain docked to the right on resize");
    checkNativeSplit("resizing with the side chat open");
    backdrop.click();
    if (openPage.querySelector(scrim) || input.parentElement.inert || document.activeElement !== input)
      throw new Error("backdrop click did not dismiss the conversation and restore board focus");
    workspace.style.width = originalWorkspaceWidth;
    window.dispatchEvent(new Event("resize"));
    setConversationVisible(true);
    conversation.dispatchEvent(new KeyboardEvent("keydown", {key: "Escape", bubbles: true, cancelable: true}));
    if (openPage.querySelector(scrim)) throw new Error("Escape did not dismiss the conversation");
    if (openPage.getBoundingClientRect().width !== fullWidth ||
        conversation.getBoundingClientRect().left !== openPage.getBoundingClientRect().left ||
        input.value !== "keep the board state" || unmounts !== 0)
      throw new Error("dismissing the drawer did not preserve the board layout and state");
    if (!sidePanel.getClientRects().length || !rightTabs.getClientRects().length ||
        sidePanel.querySelector("input").value !== "side chat draft" ||
        getComputedStyle(content).width !== content.style.width ||
        getComputedStyle(titlebar).position === "fixed")
      throw new Error("dismissing the drawer did not restore the existing native tab layout");
    toggleSideChat.click();
    const card = openPage.querySelector("div > button");
    const cardRect = card.getBoundingClientRect();
    card.focus();
    const opened = openConversation(() => Promise.resolve(), card);
    const shell = openPage.querySelector(loading);
    if (!shell || !input.parentElement.inert || openPage.style.clipPath)
      throw new Error("loading did not keep the native conversation covered");
    const motion = shell.getAnimations()[0];
    motion.pause();
    motion.currentTime = 0;
    const start = shell.getBoundingClientRect();
    motion.currentTime = 120;
    const midway = shell.getBoundingClientRect();
    motion.finish();
    const destination = shell.getBoundingClientRect();
    if (!["left", "top", "width", "height"].every(key => near(start[key], cardRect[key])) ||
        midway.width <= start.width || midway.width >= destination.width ||
        midway.height <= start.height || midway.height >= destination.height)
      throw new Error("the window did not expand from the card");
    await settle();
    if (!openPage.contains(shell)) throw new Error("the shell disappeared before the native transcript rendered");
    content.setAttribute("data-thread-find-target", "conversation");
    if (!await opened || openPage.contains(shell))
      throw new Error("the native conversation did not replace the loading shell");
    const centeredTitlebar = titlebar.getBoundingClientRect();
    const centeredSurface = conversation.getBoundingClientRect();
    if (!near((destination.left + destination.right) / 2, (boardRect.left + boardRect.right) / 2) ||
        !near((destination.top + destination.bottom) / 2, (boardRect.top + boardRect.bottom) / 2) ||
        !near(centeredTitlebar.left, destination.left) || !near(centeredTitlebar.top, destination.top) ||
        !near(centeredSurface.right, destination.right) || !near(centeredSurface.bottom, destination.bottom))
      throw new Error("the handoff moved the conversation window");
    const summaryPopup = document.querySelector("#summary-popup");
    const popupWrapper = summaryPopup.parentElement;
    summary.onclick = () => {
      const opening = summary.getAttribute("aria-expanded") === "false";
      const anchor = summary.getBoundingClientRect();
      popupWrapper.style.transform = "translate(" + anchor.left + "px," + (anchor.bottom + 8) + "px)";
      popupWrapper.hidden = false;
      summary.setAttribute("aria-expanded", String(opening));
      summaryPopup.setAttribute("data-state", opening ? "open" : "closed");
    };
    function checkSummary(withTabs) {
      const body = conversation.getBoundingClientRect();
      const chat = content.getBoundingClientRect();
      const shown = summaryPopup.getBoundingClientRect();
      const end = withTabs ? sidePanel.getBoundingClientRect().left : body.right;
      if (!near(chat.right + 8, shown.left) || !near(shown.right + 8, end) ||
          !near(shown.width, summaryPopup.firstElementChild.getBoundingClientRect().width) ||
          !isClickable(summaryPopup.firstElementChild) || !isClickable(summary) ||
          !isClickable(content.querySelector("input")))
        throw new Error("the summary did not occupy its own space inside the floating window " +
          JSON.stringify({chat, shown, end}));
      if (withTabs) {
        checkNativeSplit("opening the summary with native tabs", shown.width + 16);
        if (!near(end, mainHeader.getBoundingClientRect().right) ||
            !near(end, rightTabs.getBoundingClientRect().left))
          throw new Error("the summary misaligned the tab strip and its panel");
      }
      return shown;
    }
    summary.click();
    await settle();
    const summaryOnly = checkSummary(false);
    if (!near(summaryOnly.width, 300) ||
        !near(conversation.getBoundingClientRect().width - centeredSurface.width, summaryOnly.width + 16) ||
        !near(content.getBoundingClientRect().width, centeredSurface.width))
      throw new Error("opening just the summary did not widen the window by the summary column");
    summary.click();
    await settle();
    if (!near(conversation.getBoundingClientRect().width, centeredSurface.width) ||
        getComputedStyle(summaryPopup).visibility !== "hidden")
      throw new Error("closing the summary did not restore the conversation");
    toggleSideChat.click();
    sideRatio = 0.74;
    sizeNativePanes();
    await settle();
    checkNativeSplit("opening an oversized native tab in the centered window");
    const expanded = conversation.getBoundingClientRect();
    const split = sidePanel.getBoundingClientRect();
    if (expanded.width <= centeredSurface.width || split.width > expanded.width / 2 ||
        !near(split.left, rightHeader.getBoundingClientRect().left) ||
        !near(split.left, mainHeader.getBoundingClientRect().right) ||
        !near(split.left, rightTabs.getBoundingClientRect().left) ||
        !near(split.width, sideInner.getBoundingClientRect().width) ||
        !near(split.width, rightTabsInner.getBoundingClientRect().width))
      throw new Error("the centered split did not rebalance its body and titlebar");
    summary.click();
    await settle();
    checkSummary(true);
    if (getComputedStyle(content).scrollbarWidth !== "none" ||
        getComputedStyle(sideInner.firstElementChild).scrollbarWidth === "none")
      throw new Error("the main transcript scrollbar was not hidden independently of the native tab");
    const longTranscript = document.createElement("div");
    longTranscript.style.height = "260px";
    content.append(longTranscript);
    content.style.height = "90px";
    content.style.overflowY = "auto";
    content.scrollTop = 100;
    if (content.scrollTop !== 100 || content.clientWidth !== content.offsetWidth)
      throw new Error("a hidden scrollbar blocked scrolling or occupied space in the transcript");
    longTranscript.remove();
    content.style.removeProperty("height");
    content.style.removeProperty("overflow-y");
    if (conversation.getBoundingClientRect().width <= expanded.width)
      throw new Error("the summary did not use the remaining window width beside native tabs");
    workspace.style.width = "900px";
    window.dispatchEvent(new Event("resize"));
    await settle();
    if (checkSummary(true).width >= summaryOnly.width)
      throw new Error("a narrow window did not resize the summary column");
    summary.click();
    await settle();
    checkNativeSplit("resizing the centered window with native tabs open");
    if (sidePanel.getBoundingClientRect().width > conversation.getBoundingClientRect().width / 2)
      throw new Error("resizing the floating window squeezed out the main conversation");
    workspace.style.width = originalWorkspaceWidth;
    window.dispatchEvent(new Event("resize"));
    if (!near(conversation.getBoundingClientRect().width, expanded.width))
      throw new Error("closing the summary changed the native tab split at its original size");
    toggleSideChat.click();
    await settle();
    if (!near(conversation.getBoundingClientRect().width, centeredSurface.width))
      throw new Error("closing native tabs did not restore the single-pane width");
    const threadRoot = document.createElement("div");
    threadRoot.setAttribute("data-request-input-activity-root", "true");
    content.before(threadRoot);
    threadRoot.append(content);
    const inlineSummary = document.createElement("aside");
    inlineSummary.style.cssText = "position:absolute;top:60px;right:0;width:308px;padding-right:8px;box-sizing:border-box";
    inlineSummary.innerHTML = '<div data-pip-obstacle="thread-summary-panel" style="width:300px;max-width:100%">Inline summary</div>';
    threadRoot.after(inlineSummary);
    summary.removeAttribute("aria-expanded");
    summary.setAttribute("aria-pressed", "true");
    await settle();
    const inlineCard = inlineSummary.firstElementChild;
    const inlineBounds = inlineCard.getBoundingClientRect();
    if (!near(conversation.getBoundingClientRect().width, centeredSurface.width + 316) ||
        !near(inlineBounds.left, threadRoot.getBoundingClientRect().right + 8) ||
        !near(inlineBounds.right, conversation.getBoundingClientRect().right - 8) ||
        !isClickable(inlineCard))
      throw new Error("the inline summary did not reserve space beside the chat");
    summary.setAttribute("aria-pressed", "false");
    inlineSummary.remove();
    threadRoot.replaceWith(content);
    await settle();
    if (!near(conversation.getBoundingClientRect().width, centeredSurface.width))
      throw new Error("closing the inline summary did not restore the single-pane width");
    openPage.querySelector(scrim).click();
    if (document.activeElement !== card) throw new Error("closing did not focus the originating card");
    if (getComputedStyle(content).scrollbarWidth === "none")
      throw new Error("closing the floating window did not restore the native scrollbar");
    content.removeAttribute("data-thread-find-target");
    const composer = content.querySelector("input");
    composer.setAttribute("data-thread-find-composer", "");
    const emptyConversation = openConversation(() => Promise.resolve(), card);
    openPage.querySelector(loading).getAnimations()[0].finish();
    if (!await emptyConversation) throw new Error("a conversation with only a composer did not open");
    openPage.querySelector(scrim).click();
    composer.removeAttribute("data-thread-find-composer");
    let completeNavigation;
    const canceled = openConversation(() => new Promise(resolve => { completeNavigation = resolve; }), card);
    openPage.querySelector(loading).getAnimations()[0].finish();
    openPage.querySelector(scrim).click();
    completeNavigation();
    if (await canceled || openPage.querySelector(loading) || openPage.querySelector(scrim))
      throw new Error("a dismissed conversation reopened after navigation completed");
    const navigationError = new Error("navigation failed");
    const failure = await openConversation(() => Promise.reject(navigationError), card).catch(error => error);
    if (failure !== navigationError || openPage.querySelector(loading) || input.parentElement.inert)
      throw new Error("a failed navigation did not restore the board");
    row.hidden = false;
    await check(row, "visible row did not inherit one launcher");
    if (document.querySelector(page) !== openPage) throw new Error("changing mounts replaced the page");
    row.style.visibility = "hidden";
    await check(rail, "invisible row did not return the launcher to the rail");
    row.style.visibility = "visible";
    row.setAttribute("aria-hidden", "true");
    await check(rail, "aria-hidden row was treated as usable");
    rail.setAttribute("inert", "");
    await check(null, "inert rail retained an inaccessible launcher");
    rail.removeAttribute("inert");
    await check(rail, "restored rail did not regain the launcher");
    setConversationVisible(true);
    host.stop();
    if (unmounts !== 1 || getComputedStyle(conversation).position === "fixed" || document.querySelector(scrim))
      throw new Error("host stop did not restore the original native layout");
    window.dispatchEvent(new Event("resize"));
    await check(null, "stopped host retained a launcher");
    result.textContent = "passed";
  } catch (error) {
    result.textContent = error.message;
  }
</script>`;

const runtimeFiles = new Map(
    await Promise.all(
        ["host.js", "registry.js"].map(async (name) => [name, await readFile(join(runtime, name))]),
    ),
);
const server = createServer((request, response) => {
    const asset = runtimeFiles.get(request.url?.slice(1));
    if (request.url === "/") response.setHeader("Content-Type", "text/html").end(fixture);
    else if (asset) response.setHeader("Content-Type", "text/javascript").end(asset);
    else response.writeHead(404).end();
});
const profile = await mkdtemp(join(tmpdir(), "enhancer-mount-"));
try {
    await new Promise((ready, fail) => server.once("error", fail).listen(0, "127.0.0.1", ready));
    const url = `http://127.0.0.1:${server.address().port}/`;
    const { stdout } = await promisify(execFile)(
        browser,
        [
            "--headless",
            "--dump-dom",
            "--no-proxy-server",
            "--window-size=1600,850",
            "--virtual-time-budget=2500",
            `--user-data-dir=${profile}`,
            url,
        ],
        { timeout: 20_000 },
    );
    const result = stdout.match(/<output id="result">([^<]*)<\/output>/u)?.[1];
    assert.equal(result, "passed");
    console.log(JSON.stringify({ runtime, browser, result }));
} finally {
    await new Promise((done) => server.close(done));
    await rm(profile, { force: true, recursive: true });
}
