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
<main data-app-shell-workspace style="min-height:200px"></main>
<output id="result">waiting</output>
<script type="module">
  const row = document.querySelector("#row");
  const rail = document.querySelector("#rail");
  const result = document.querySelector("#result");
  const item = '[data-codex-enhancer-item="session-board"]';
  const page = '[data-codex-enhancer-page="session-board"]';
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
    const host = runtime.startUiHost({
      async fetchState() {
        return {items: [{id: "session-board", title: "Session Board", route: "/session-board",
          page: {title: "Session Board", body: "Fixture"}}], autoOpen: false};
      },
      subscribe() {},
      proof() {},
    });
    await check(rail, "initial hidden row did not fall back to the visible rail");
    document.querySelector(item).click();
    const openPage = document.querySelector(page);
    if (!openPage) throw new Error("rail launcher did not open the page");
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
    host.stop();
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
            "--virtual-time-budget=2500",
            `--user-data-dir=${profile}`,
            url,
        ],
        { timeout: 20_000 },
    );
    const result = stdout.match(/<output id="result">([^<]*)<\/output>/u)?.[1];
    assert.equal(result, "passed");
    console.log(JSON.stringify({ runtime, browser, checks: 8, result }));
} finally {
    await new Promise((done) => server.close(done));
    await rm(profile, { force: true, recursive: true });
}
