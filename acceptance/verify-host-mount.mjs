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
    <header data-app-shell-titlebar style="display:flex;height:36px">
      <button id="toggle-side-chat">Side chat</button>
      <div data-app-shell-tab-strip-controller="right" hidden>
        <button id="preview-tab">Existing Codex tab</button>
      </div>
    </header>
  </div>
  <div data-app-shell-workspace style="position:relative;isolation:isolate;display:flex;height:600px;width:1464px;overflow:hidden">
    <main data-app-shell-main-surface tabindex="-1" style="flex:1;min-width:0">
    <div style="display:flex">
      <div data-app-shell-main-content-layout="thread-edge-scroll" style="display:flex;flex-direction:column;flex:1;min-width:0;overflow:hidden">
        <div id="conversation" style="align-self:end;min-width:100%">
          <input aria-label="Main chat message"><button>Send</button>
        </div>
      </div>
      <aside data-app-shell-focus-area="right-panel" style="flex-shrink:0;height:240px" hidden>
        <input aria-label="Side chat message"><button id="side-send">Send side chat</button>
      </aside>
    </div>
    </main>
  </div>
</div>
<output id="result">waiting</output>
<script type="module">
  const row = document.querySelector("#row");
  const rail = document.querySelector("#rail");
  const result = document.querySelector("#result");
  const item = '[data-codex-enhancer-item="session-board"]';
  const page = '[data-codex-enhancer-page="session-board"]';
  const scrim = "[data-codex-enhancer-conversation-scrim]";
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
    let unmounts = 0;
    const host = runtime.startUiHost({
      async fetchState() {
        return {items: [{id: "session-board", title: "Session Board", route: "/session-board",
          page: {title: "Session Board", body: "Fixture"}}], autoOpen: false};
      },
      subscribe() {},
      proof() {},
      mountPage(_id, panel, _close, showConversation) {
        setConversationVisible = showConversation;
        const content = document.createElement("div");
        content.append(document.createElement("input"));
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
    const rightTabs = document.querySelector("[data-app-shell-tab-strip-controller=right]");
    const titlebar = document.querySelector("[data-app-shell-titlebar]");
    const toggleSideChat = titlebar.querySelector("button");
    const previewTab = rightTabs.querySelector("button");
    let sideRatio = 0.4;
    function sizeNativePanes() {
      const workspaceWidth = workspace.clientWidth;
      const sideWidth = Math.round(workspaceWidth * sideRatio);
      sidePanel.style.width = sideWidth + "px";
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
    if (!isClickable(toggleSideChat)) {
      const bounds = toggleSideChat.getBoundingClientRect();
      const hit = document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
      throw new Error("native titlebar controls are covered by " + hit?.outerHTML.slice(0, 170));
    }
    function checkNativeSplit(label) {
      const surface = conversation.getBoundingClientRect();
      const mainBounds = content.getBoundingClientRect();
      const sideBounds = sidePanel.getBoundingClientRect();
      if (mainBounds.left !== surface.left || mainBounds.right !== sideBounds.left)
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
