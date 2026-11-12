import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

const runtimeIdentity = value => Object.fromEntries(
  ["pid", "build", "version", "dataDir", "configId", "app"]
    .filter(key => Object.hasOwn(value, key) && ["string", "number"].includes(typeof value[key])).map(key => [key, value[key]]));
async function developmentIdentity(url) {
  const registry = path.join(os.homedir(), ".csb-dev"), matches = [];
  for (const entry of await fs.readdir(registry, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const root = path.join(registry, entry.name), recordPath = path.join(root, "runtime.json");
    const directory = await fs.lstat(root);
    if (!directory.isDirectory() || directory.uid !== process.getuid()) continue;
    let record;
    try {
      const metadata = await fs.lstat(recordPath);
      if (!metadata.isFile() || metadata.uid !== process.getuid()) continue;
      record = JSON.parse(await fs.readFile(recordPath, "utf8"));
    } catch (error) { if (error.code === "ENOENT" || error instanceof SyntaxError) continue; throw error; }
    if (record?.phase !== "running" || record.root !== root) continue;
    try { if (new URL(record.url).href !== url) continue; } catch { continue; }
    assert(await fs.realpath(root) === root, "开发记录目录不能指向其他路径");
    assert(Number.isSafeInteger(record.service_pid) && record.service_pid > 0 &&
      /^[a-f0-9]{64}$/.test(record.program_sha256), "开发记录缺少有效进程或构建身份");
    matches.push({ root, pid: record.service_pid, build: record.program_sha256 });
  }
  assert(matches.length === 1, "URL 必须唯一匹配当前用户的 running 开发记录");
  const expected = matches[0], dataDir = path.join(expected.root, "data");
  assert(await fs.realpath(dataDir) === dataDir, "开发数据目录不能指向其他路径");
  const response = await fetch(new URL("api/info", url), { redirect: "error", signal: AbortSignal.timeout(5000) });
  assert(response.ok, "无法读取开发服务身份");
  let info;
  try { info = await response.json(); } catch { throw new Error("开发服务身份响应不是有效 JSON"); }
  assert(info && info.pid === expected.pid && info.build === expected.build && info.dataDir === dataDir,
    "开发记录与服务 pid/build/dataDir 不匹配，拒绝启动浏览器");
  return { root: expected.root, url, ...runtimeIdentity(info) };
}

const verifyOnly = process.argv.includes("--verify-only");
const argumentsList = process.argv.slice(2).filter(argument => argument !== "--verify-only");
assert.equal(argumentsList.length, 3, "用法：node scripts/check-dev-isolation.mjs URL_A URL_B OUTPUT [--verify-only]");
const urls = argumentsList.slice(0, 2).map(value => {
  const url = new URL(value);
  assert(url.protocol === "http:" && url.hostname === "127.0.0.1" && !url.username && !url.password &&
    url.pathname === "/" && !url.search && !url.hash, "只接受 http://127.0.0.1:PORT/");
  return url.href;
});
assert.notEqual(urls[0], urls[1], "必须提供两个不同的开发环境");
const output = path.resolve(argumentsList[2]);
await fs.mkdir(output, { recursive: true });
const report = { passed: false, mode: verifyOnly ? "verify-only" : "write", urls, browsers: [], results: [], cleanup: [], errors: [] };
const browsers = [], pages = [];
const until = async (operation, message) => {
  let last;
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    try { const value = await operation(); if (value) return value; } catch (error) { last = error.message; }
    await delay(100);
  }
  throw new Error(`${message}${last ? `：${last}` : ""}`);
};
const snapshot = page => page.evaluate(`(async () => {
  const response = await fetch("/api/sessions", {cache:"no-store"});
  if (!response.ok) throw new Error("读取看板失败：" + response.status);
  return response.json();
})()`);
const row = (view, id) => {
  const session = view.sessions.find(candidate => candidate.id === id);
  assert(session, `会话未发现：${id}`);
  return session;
};
const fields = session => ({ state: session.state, note: session.note });
const point = (page, selector) => page.evaluate(`(() => {
  const node = document.querySelector(${JSON.stringify(selector)});
  if (!node) throw new Error("元素未找到");
  node.scrollIntoView({block:"center",inline:"nearest",behavior:"instant"});
  const rect = node.getBoundingClientRect();
  return {x:rect.left+rect.width/2,y:rect.top+Math.min(rect.height/2,40)};
})()`);
const click = async (page, selector) => {
  const position = await point(page, selector);
  await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...position, button: "left", buttons: 1, clickCount: 1 });
  await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...position, button: "left", clickCount: 1 });
};
const drag = async (page, id, state) => {
  await page.command("Input.setInterceptDrags", { enabled: true });
  try {
    const source = await point(page, `.card[data-id="${id}"] .card-title`);
    const target = await point(page, `#column-${state} .cards`);
    const intercepted = page.event("Input.dragIntercepted");
    intercepted.catch(() => {});
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", ...source });
    await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...source, button: "left", buttons: 1, clickCount: 1 });
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", x: source.x + 30, y: source.y + 15, buttons: 1, button: "left" });
    const { data } = await intercepted;
    assert(data.items.some(item => item.mimeType === "text/plain" && item.data === id), "未捕获真实卡片拖拽");
    for (const type of ["dragEnter", "dragOver", "drop"])
      await page.command("Input.dispatchDragEvent", { type, ...target, data });
    await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...target, button: "left", clickCount: 1 });
  } finally { await page.command("Input.setInterceptDrags", { enabled: false }); }
};
const verify = async (page, id, expected) => {
  const view = await until(async () => {
    const current = await snapshot(page);
    return JSON.stringify(fields(row(current, id))) === JSON.stringify(expected) && current;
  }, `状态或备注未保存：${JSON.stringify(expected)}`);
  await page.evaluate("load()");
  await until(() => page.evaluate(`(() => {
    const card = document.querySelector('.card[data-id="${id}"]');
    return card?.closest(".column").dataset.state === ${JSON.stringify(expected.state)} &&
      (card.querySelector(".card-note")?.textContent || "") === ${JSON.stringify(expected.note)};
  })()`), "页面未显示实际保存结果");
  return { ...fields(row(view, id)), version: row(view, id).version, runtime: runtimeIdentity(view.runtime) };
};
try {
  const previous = verifyOnly ? JSON.parse(await fs.readFile(path.join(output, "isolation.json"), "utf8")) : null;
  if (previous) assert(previous.passed && previous.sessionId && previous.expected?.length === 2, "缺少成功的首次检查记录");
  report.identities = await Promise.all(urls.map(developmentIdentity));
  assert.notEqual(report.identities[0].dataDir, report.identities[1].dataDir, "两个开发记录共用数据目录");
  for (const url of urls) {
    const browser = await launchBrowser();
    browsers.push(browser);
    report.browsers.push({ pid: browser.pid, profile: browser.profile, port: browser.port, version: await browser.command("Browser.getVersion") });
    const page = await browser.page(url);
    pages.push(page);
    await until(() => page.evaluate(`typeof data !== "undefined" && document.querySelector(".card") &&
      data.health.runtimeVerified && data.health.nativeVerified && data.health.scanComplete`), "真实看板未就绪");
  }
  assert.notEqual(browsers[0].profile, browsers[1].profile);
  assert.notEqual(browsers[0].port, browsers[1].port);
  const before = await Promise.all(pages.map(snapshot));
  assert.notEqual(before[0].runtime.dataDir, before[1].runtime.dataDir, "两个看板共用数据库目录");
  const id = previous?.sessionId || before[0].sessions.find(session => !session.readOnly && !session.remote &&
    before[1].sessions.some(other => other.id === session.id && !other.readOnly && !other.remote))?.id;
  assert(/^[0-9a-f-]{36}$/.test(id || ""), "需要两个看板中相同的本地会话 UUID");
  const expected = previous?.expected || ["review", "to_archive"].map((state, index) =>
    ({ state, note: `dev-isolation-${index ? "B" : "A"}-${Date.now()}` }));
  Object.assign(report, { sessionId: id, expected, before: before.map(view => ({
    session: { id, ...fields(row(view, id)), version: row(view, id).version },
    runtime: runtimeIdentity(view.runtime) })) });
  for (const [index, page] of pages.entries()) {
    if (previous) assert.equal(before[index].runtime.dataDir, previous.results[index].runtime.dataDir, "复验目标环境改变");
    else {
      if (row(before[index], id).state === "blocked") {
        await drag(page, id, "unknown");
        await verify(page, id, { state: "unknown", note: row(before[index], id).note });
      }
      await drag(page, id, "blocked");
      await until(() => page.evaluate('document.querySelector("#editor").open'), "拖入阻塞列未打开真实表单");
      await page.evaluate('document.querySelector("#note").focus();document.querySelector("#note").select()');
      await page.command("Input.insertText", { text: expected[index].note });
      await click(page, "#save");
      await until(() => page.evaluate('!document.querySelector("#editor").open && !saving'), "真实表单保存未完成");
      await verify(page, id, { state: "blocked", note: expected[index].note });
      await drag(page, id, expected[index].state);
      await verify(page, id, expected[index]);
      const other = 1 - index;
      assert.deepEqual(fields(row(await snapshot(pages[other]), id)), index ? expected[other] : fields(row(before[other], id)),
        "操作一个看板改变了另一个看板");
      const navigation = await page.evaluate(`request("/api/open",${JSON.stringify({ id })})`);
      assert.equal(navigation.ok, true);
      assert.equal(new URL(navigation.url).pathname, `/${id}`);
      report.results.push({ navigation: { ok: navigation.ok, url: navigation.url } });
    }
    await Promise.all([page.event("Page.loadEventFired"), page.command("Page.reload", { ignoreCache: true })]);
    const result = await verify(page, id, expected[index]);
    report.results[index] = { ...report.results[index], ...result };
  }
  await Promise.all(pages.map((page, index) => verify(page, id, expected[index])));
  report.passed = true;
} catch (error) { report.errors.push(error.stack || String(error)); }
finally {
  for (const [index, page] of pages.entries()) {
    try {
      const image = await page.command("Page.captureScreenshot", { format: "png" });
      const filename = `${verifyOnly ? "verify-" : ""}${index ? "b" : "a"}.png`;
      await fs.writeFile(path.join(output, filename), Buffer.from(image.data, "base64"));
      (report.screenshots ||= []).push(filename);
    } catch (error) { report.passed = false; report.errors.push(`截图失败：${error.message}`); }
  }
  for (const browser of browsers) {
    try { report.cleanup.push(await browser.close()); }
    catch (error) { report.passed = false; report.errors.push(`Chrome 清理失败：${error.message}`); }
  }
  await fs.writeFile(path.join(output, verifyOnly ? "verification.json" : "isolation.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ passed: report.passed, mode: report.mode, output, errors: report.errors }));
  if (!report.passed) process.exitCode = 1;
}
