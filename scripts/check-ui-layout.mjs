import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const output = process.argv[2] || await fs.mkdtemp(path.join(os.tmpdir(), "board-layout-"));
await fs.mkdir(output, { recursive: true });
const html = await fs.readFile(process.env.CODEX_BOARD_TEST_HTML || path.join(root, "index.html"), "utf8");
const states = { unknown: "待判断", in_progress: "进行中", review: "待审阅", blocked: "已阻塞", to_archive: "待归档" };
const fixture = {
  states, health: {}, projectCatalog: { loaded: true, items: [
    { id: "first", label: "第一个项目", paths: ["/projects/first"] },
    { id: "second", label: "第二个项目", paths: ["/projects/second"] },
  ] },
  sessions: ["unknown", "review", "to_archive"].map((state, index) => ({
    id: `layout-${index}`, title: "基线标题", state, projectId: index ? "second" : "first",
    cwd: "/projects/example", note: "", updatedAt: 1760000000, source: "codex",
    version: 1, inputVersion: 1, sectionVersion: "fixture", canArchive: false,
  })),
};
const server = http.createServer((request, response) => {
  if (request.method !== "GET") return response.writeHead(405).end();
  const api = request.url === "/api/sessions";
  response.writeHead(200, { "Content-Type": api ? "application/json" : "text/html; charset=utf-8" });
  response.end(api ? JSON.stringify(fixture) : html);
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const candidates = process.env.CODEX_BOARD_BROWSER ? [process.env.CODEX_BOARD_BROWSER] :
  ["google-chrome", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
const browser = candidates.find(candidate => spawnSync(candidate, ["--version"]).status === 0);
assert(browser, "需要 Chrome/Chromium；可用 CODEX_BOARD_BROWSER 指定可执行文件");
assert.equal(typeof WebSocket, "function", "浏览器检查需要 Node.js 22 或更新版本");
const profile = await fs.mkdtemp(path.join(os.tmpdir(), "board-chrome-"));
const chrome = spawn(browser, ["--headless=new", "--no-sandbox", "--disable-gpu",
  "--disable-dev-shm-usage", "--no-first-run", "--no-default-browser-check",
  "--disable-background-networking", "--disable-extensions", "--remote-debugging-port=0",
  `--user-data-dir=${profile}`, "about:blank"], { stdio: "ignore" });
let socket;
try {
  let endpoint;
  for (let attempt = 0; attempt < 100 && !endpoint; attempt++) {
    try { endpoint = (await fs.readFile(path.join(profile, "DevToolsActivePort"), "utf8")).split("\n"); }
    catch { await new Promise(resolve => setTimeout(resolve, 100)); }
  }
  assert(endpoint, "Chrome 调试端点未启动");
  socket = new WebSocket(`ws://127.0.0.1:${endpoint[0]}${endpoint[1]}`);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let serial = 0;
  const pending = new Map();
  socket.addEventListener("message", event => {
    const message = JSON.parse(String(event.data));
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    clearTimeout(waiter.timer);
    message.error ? waiter.reject(new Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
  });
  const command = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++serial;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP 超时：${method}`)); }, 10000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });
  const target = await command("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await command("Target.attachToTarget", { targetId: target.targetId, flatten: true });
  const evaluate = async expression => {
    const result = await command("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const resize = width => command("Emulation.setDeviceMetricsOverride",
    { width, height: 1000, deviceScaleFactor: 1, mobile: false }, sessionId);
  await resize(1474);
  await command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` }, sessionId);
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await evaluate("Boolean(document.querySelector('.card'))")) break;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  const measurements = [];
  const measure = async name => {
    const result = await evaluate(`(() => {
      const viewport=byId("board-viewport"), columns=[...document.querySelectorAll(".column")];
      return {window:innerWidth,viewport:viewport.clientWidth,scroll:viewport.scrollWidth,
        documentWidth:document.documentElement.scrollWidth,columns:columns.map(node=>node.getBoundingClientRect().width),
        count:document.querySelectorAll(".card").length,titles:[...document.querySelectorAll(".card-title")].map(node=>({
          width:node.clientWidth,scroll:node.scrollWidth,height:node.getBoundingClientRect().height,
          lineHeight:parseFloat(getComputedStyle(node).lineHeight)}))};
    })()`);
    assert.equal(result.columns.length, fixture.legacyMode ? 6 : 5, name);
    assert(Math.max(...result.columns) - Math.min(...result.columns) < 1, `${name}: 列宽不一致`);
    assert(result.documentWidth <= result.window, `${name}: 横向滚动泄漏到页面`);
    for (const title of result.titles) {
      assert(title.scroll <= title.width + 1, `${name}: 标题未在卡片内换行`);
      assert(title.height <= title.lineHeight * 2 + 1, `${name}: 标题超过两行`);
    }
    measurements.push({ name, ...result });
    return result;
  };
  const select = value => evaluate(`byId("project-filter").value=${JSON.stringify(value)};
    byId("project-filter").dispatchEvent(new Event("change"))`);
  for (const legacy of [false, true]) {
    fixture.legacyMode = legacy;
    fixture.states = legacy ? { ...states, completed: "已完成" } : states;
    await resize(1474);
    await evaluate(`data=${JSON.stringify(fixture)};selectedHost="local";selectedProject="all";filterMode="project";projectOptionsKey="";render()`);
    const baseline = await measure(`${legacy ? "six" : "five"}-short`);
    const long = structuredClone(fixture);
    long.sessions[1].title = "长标题与不可断开的文本".repeat(40) + "x".repeat(400);
    long.sessions[1].cwd = "/projects/" + "very-long-path/".repeat(50);
    long.sessions[1].note = "很长的备注".repeat(100);
    long.projectCatalog.items[1].label = "长项目标签".repeat(80);
    await evaluate(`data=${JSON.stringify(long)};projectOptionsKey="";render()`);
    const expanded = await measure(`${legacy ? "six" : "five"}-long`);
    assert(Math.abs(baseline.columns[0] - expanded.columns[0]) < 1, "长内容改变列宽");
    assert.equal(expanded.scroll, baseline.scroll, "长内容扩大看板");
    for (const value of ["project:first", "project:second", "all"]) {
      await select(value);
      const filtered = await measure(`${legacy ? "six" : "five"}-${value}`);
      assert.equal(filtered.count, value === "all" ? 3 : value === "project:first" ? 1 : 2, "项目筛选数量不符");
      assert(Math.abs(filtered.columns[0] - baseline.columns[0]) < 1, "项目筛选改变列宽");
    }
    await evaluate(`byId("filter-toggle").click();byId("search").value="不存在的搜索内容";
      byId("search").dispatchEvent(new Event("input"))`);
    const empty = await measure(`${legacy ? "six" : "five"}-empty-search`);
    assert.equal(empty.count, 0, "搜索没有清空卡片");
    assert(Math.abs(empty.columns[0] - baseline.columns[0]) < 1, "空搜索改变列宽");
    await evaluate(`byId("filter-toggle").click();render()`);
    for (const width of [801, 800, 600, 390]) {
      await resize(width);
      const narrow = await measure(`${legacy ? "six" : "five"}-${width}`);
      assert(narrow.scroll > narrow.viewport, "窄屏没有横向滚动");
      assert(narrow.columns.every(size => size >= (width > 800 ? 240 : 270)), "窄屏列被挤压");
      assert(await evaluate(`(() => { const viewport=byId("board-viewport");
        viewport.scrollLeft=viewport.scrollWidth;const last=byId("board").lastElementChild;
        return last.getBoundingClientRect().right<=viewport.getBoundingClientRect().right+1; })()`),
      "横向滚动无法到达最后一列");
    }
    await resize(1474);
    const image = await command("Page.captureScreenshot", { format: "png" }, sessionId);
    await fs.writeFile(path.join(output, `${legacy ? "six" : "five"}-columns.png`), Buffer.from(image.data, "base64"));
  }
  const report = { browser: await command("Browser.getVersion"), cases: measurements.length, measurements };
  await fs.writeFile(path.join(output, "measurements.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ cases: report.cases, output }));
} finally {
  socket?.close();
  chrome.kill("SIGTERM");
  await new Promise(resolve => chrome.exitCode !== null || chrome.signalCode !== null ? resolve() : chrome.once("exit", resolve));
  await new Promise(resolve => server.close(resolve));
  await fs.rm(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 50 });
}
