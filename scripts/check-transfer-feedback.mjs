import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

function installClock() {
  let now = 0, nextId = 0;
  const timers = new Map();
  const schedule = (callback, delay, interval, args) => {
    if (typeof callback !== "function") throw new Error("Test clock requires function callbacks");
    const duration = Math.max(interval ? 1 : 0, Number(delay) || 0), id = ++nextId;
    timers.set(id, { id, at: now + duration, interval: interval ? duration : 0, callback, args });
    return id;
  };
  window.setTimeout = (callback, delay = 0, ...args) => schedule(callback, delay, false, args);
  window.setInterval = (callback, delay = 0, ...args) => schedule(callback, delay, true, args);
  window.clearTimeout = window.clearInterval = id => timers.delete(id);
  Object.defineProperty(performance, "now", { configurable: true, value: () => now });
  window.transferTestClock = { advance(milliseconds) {
    if (!Number.isFinite(milliseconds) || milliseconds < 0) throw new Error("Invalid test time");
    const target = now + milliseconds;
    let executed = 0;
    for (;;) {
      const next = [...timers.values()].filter(timer => timer.at <= target)
        .sort((first, second) => first.at - second.at || first.id - second.id)[0];
      if (!next) break;
      if (++executed > 10000) throw new Error("Test clock callback limit exceeded");
      now = next.at;timers.delete(next.id);
      if (next.interval) timers.set(next.id, { ...next, at: now + next.interval });
      next.callback.apply(window, next.args);
    }
    now = target;
  } };
}

const output = process.argv[2] || await fs.mkdtemp(path.join(os.tmpdir(), "board-transfer-"));
await fs.mkdir(output, { recursive: true });
const html = await fs.readFile(new URL("../index.html", import.meta.url), "utf8");
const snapshot = { states: { unknown: "待判断", in_progress: "进行中", review: "待审阅", blocked: "已阻塞", to_archive: "待归档" },
  health: { remoteRegistry: [{ hostId: "fixture", hostLabel: "测试远端", enabled: true }, { hostId: "other", hostLabel: "另一主机", enabled: true }] },
  projectCatalog: { loaded: true, items: [
    { id: "project", label: "测试项目", paths: ["/fixture"], remote: true, hostId: "fixture" },
    { id: "other-project", label: "另一项目", paths: ["/other"], remote: true, hostId: "other" },
  ] },
  sessions: [
    { id: 'remote:["codex","fixture","thread"]', title: "远程长标题".repeat(12), state: "in_progress", remote: true, hostId: "fixture", hostLabel: "测试远端", projectId: "project" },
    { id: "local", title: "本机会话", state: "unknown", projectId: null },
    { id: 'remote:["codex","other","archive-thread"]', title: "另一主机待归档会话", state: "to_archive", remote: true, hostId: "other", hostLabel: "另一主机", projectId: "other-project", canArchive: true },
  ].map(session => ({ note: "保留原备注", reason: "", version: 3, inputVersion: 5, sectionVersion: "native:source", canArchive: false, ...session })) };
const writes = [], opens = [], mutations = [], responses = [], cases = [];
let snapshotError = false, refreshError = false, automatic = false, holdSnapshot = false, heldSnapshot, pendingNew;
const complete = async (status = 200, apply = status === 200, titlePending = false) => {
  for (let attempt = 0; attempt < 150 && !responses.length; attempt++) await delay(40);
  const { response, body } = responses.shift() || assert.fail("等待保存请求送达超时");
  if (apply) Object.assign(snapshot.sessions.find(row => row.id === body.id),
    { state: body.state, note: body.note, version: body.version + 1, sectionVersion: "native:" + body.state, titleSyncPending: titlePending });
  response.writeHead(status, { "Content-Type": "application/json" });
  response.end(JSON.stringify(status === 200 ? { ok: true, titleEnabled: true, titlePending } : { error: "版本冲突或连接未确认" }));
};
const server = http.createServer(async (request, response) => {
  if (request.url === "/") { response.setHeader("Content-Type", "text/html; charset=utf-8");response.end(html);return; }
  if (request.url === "/api/sessions") {
    if (holdSnapshot) {holdSnapshot = false;heldSnapshot = { response, text: JSON.stringify(snapshot) };return;}
    response.writeHead(snapshotError ? 503 : 200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(snapshotError ? { error: "读取断线" } : snapshot));return;
  }
  let text = "";
  for await (const chunk of request) text += chunk;
  const body = JSON.parse(text || "{}");
  if (request.url === "/api/state") {
    writes.push(body);responses.push({ response, body });if (automatic) await complete();return;
  }
  if (request.url === "/api/open") opens.push(body.id);
  if (["/api/archive", "/api/column-sort", "/api/new", "/api/new/reset"].includes(request.url)) mutations.push({ path: request.url, body });
  if (request.url === "/api/new") {pendingNew = response;return;}
  if (request.url === "/api/refresh" && refreshError) {
    response.writeHead(503, { "Content-Type": "application/json" });response.end('{"error":"刷新断线"}');return;
  }
  response.setHeader("Content-Type", "application/json");response.end('{"ok":true}');
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const browser = await launchBrowser();
const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
const initialReducedMotion = await page.evaluate('matchMedia("(prefers-reduced-motion: reduce)").matches');
await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await page.command("Page.addScriptToEvaluateOnNewDocument", { source: `(${installClock.toString()})()` });
await page.command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` });
const evaluate = expression => page.evaluate(expression);
const advance = milliseconds => evaluate(`transferTestClock.advance(${milliseconds})`);
const advanceTo = async elapsed => {
  const age = await evaluate("performance.now() - moving.started");
  assert(age <= elapsed, `测试时钟已超过检查点：${age} > ${elapsed}`);
  await advance(elapsed - age);
};
const until = async expression => {
  for (let attempt = 0; attempt < 150; attempt++) {
    if (await evaluate(expression)) return;
    await delay(40);
  }
  throw new Error("等待条件超时：" + expression);
};
const check = async (name, expression) => {
  assert(await evaluate(expression), name);cases.push(name);
};
const select = (id, value) => evaluate(`byId(${JSON.stringify(id)}).value=${JSON.stringify(value)};byId(${JSON.stringify(id)}).dispatchEvent(new Event("change"))`);
const drag = async (id, selector) => {
  const point = selector => evaluate(`(() => {
    const node = document.querySelector(${JSON.stringify(selector)});node.scrollIntoView({block:"center",inline:"nearest"});
    const rect = node.getBoundingClientRect();return {x:rect.left+rect.width/2,y:rect.top+Math.min(30,rect.height/2)};
  })()`);
  const source = await point(`.card[data-id=${JSON.stringify(id)}] .card-title`), target = await point(selector);
  await page.command("Input.setInterceptDrags", { enabled: true });
  try {
    const intercepted = page.event("Input.dragIntercepted");
    intercepted.catch(() => {});
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", ...source });
    await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...source, button: "left", buttons: 1, clickCount: 1 });
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", x: source.x + 30, y: source.y + 15, buttons: 1, button: "left" });
    const { data } = await intercepted;
    assert(data.items.some(item => item.data === id), "必须捕获真实卡片拖拽");
    for (const type of ["dragEnter", "dragOver", "drop"]) await page.command("Input.dispatchDragEvent", { type, ...target, data });
    await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...target, button: "left", clickCount: 1 });
  } finally { await page.command("Input.setInterceptDrags", { enabled: false }); }
};
const settled = () => until("!saving && !moving");
const move = state => evaluate(`void moveCard(data.sessions[0],${JSON.stringify(state)})`);
const screenshot = async name => {
  const image = await page.command("Page.captureScreenshot", { format: "png" });
  await fs.writeFile(path.join(output, name + ".png"), Buffer.from(image.data, "base64"));
};
try {
  await until('typeof transferTestClock !== "undefined" && Boolean(document.querySelector(".card"))');
  const normalReducedMotion = await evaluate('matchMedia("(prefers-reduced-motion: reduce)").matches');
  console.error(JSON.stringify({ motion: { initialReducedMotion, normalReducedMotion } }));
  assert.equal(normalReducedMotion, false);
  assert.deepEqual(await evaluate("moveDelays"), { courier: 1500, slow: 8000 });
  const id = snapshot.sessions[0].id, otherId = snapshot.sessions[2].id;
  await evaluate('window.pendingCreation=createSession("project");void 0');
  await until('creating');
  await check("新建等待中禁用拖动归档排序与筛选", 'byId("new-session").disabled && byId("host-filter").disabled && document.querySelector("[data-archive-all]").disabled && [...document.querySelectorAll(".card")].every(card=>!card.draggable) && [...document.querySelectorAll(".column-sort")].every(button=>button.disabled)');
  await evaluate('(() => {const card=document.querySelector(".card"), event=new Event("dragstart",{cancelable:true});card.dispatchEvent(event);window.creationBlockedDrag=event.defaultPrevented;void moveCard(data.sessions[0],"review");void archiveSessions([data.sessions[2]],document.querySelector("[data-archive-all]"));void confirmArchiveInputs(data.sessions[2],document.querySelector("[data-archive-all]"));void changeColumnSort("to_archive",document.querySelector("#column-to_archive .column-sort"));byId("draft-restart").onclick();openEditor(data.sessions[0].id,"blocked");byId("edit-form").onsubmit({preventDefault(){}});})()');
  await check("新建先开始时各写入口也保持互斥", 'creating && !saving && !dragging && window.creationBlockedDrag && !byId("editor").open');
  for (let attempt = 0; attempt < 150 && !pendingNew; attempt++) await delay(40);
  assert.equal(writes.length, 0);assert.equal(mutations.length, 1);
  assert.equal(mutations[0].path, "/api/new");assert.equal(mutations[0].body.projectId, "project");
  assert.match(mutations[0].body.requestId, /^[0-9a-f-]{36}$/);
  const expectedMutations = structuredClone(mutations);
  assert(pendingNew, "必须实际挂起新建请求");
  pendingNew.setHeader("Content-Type", "application/json");pendingNew.end('{"ok":true}');
  await evaluate('window.pendingCreation');
  await check("新建结束后恢复卡片和控件", '!creating && !saving && !byId("new-session").disabled && !byId("host-filter").disabled && [...document.querySelectorAll(".card")].every(card=>card.draggable)');
  holdSnapshot = true;
  await evaluate('window.staleRead=load();void 0');
  for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
  assert(heldSnapshot, "必须实际挂起旧快照请求");
  await drag(id, "#column-review .cards");
  await check("临时落位不修改已确认记录", 'saving && data.sessions[0].state === "in_progress" && document.querySelector(".move-pending").closest(".column").dataset.state === "review" && byId("column-review").querySelector(".column-count").textContent === "0 + 1"');
  await advance(300);
  await until('document.querySelector(".move-pending").getAnimations({subtree:true}).some(animation=>animation.animationName==="move-glow")');
  const glowBoundary = await evaluate(`(() => {
    const card = document.querySelector(".move-pending");
    const animation = card.getAnimations({subtree:true}).find(item => item.animationName === "move-glow");
    animation.pause();
    const timing = animation.effect.getTiming();
    animation.currentTime = 179;
    const before = getComputedStyle(card,"::after").opacity;
    animation.currentTime = 180;
    const after = getComputedStyle(card,"::after").opacity;
    animation.currentTime = 300;
    return {delay:timing.delay,before,after};
  })()`);
  assert.deepEqual(glowBoundary, { delay: 180, before: "0", after: "1" });
  await check("保存中触发离开保护", '(() => {const event=new Event("beforeunload",{cancelable:true});window.dispatchEvent(event);return event.defaultPrevented;})()');
  await check("短等待仅有微光", 'performance.now() - moving.started === 300 && getComputedStyle(document.querySelector(".move-pending"),"::after").opacity !== "0" && byId("move-feedback").hidden');
  await screenshot("short-wait");
  await evaluate('void moveCard(data.sessions[0],"to_archive");void load();byId("refresh").click()');
  assert.equal(writes.length, 1, "重复操作不能追加写入");
  assert.deepEqual(writes[0], { id, state: "review", note: "保留原备注", version: 3, inputVersion: 5, sectionVersion: "native:source" });
  await select("host-filter", "host:other");
  await select("project-filter", "project:other-project");
  await check("等待中主机和项目筛选仍可使用", 'selectedHost === "host:other" && selectedProject === "project:other-project" && !byId("host-filter").disabled && !byId("project-filter").disabled && !document.querySelector(".move-pending") && visibleSessions().length === 1');
  await evaluate('byId("filter-toggle").click();byId("search").value="另一主机";byId("search").dispatchEvent(new Event("input"))');
  await check("等待中搜索保持所选主机范围", 'filterMode === "search" && !byId("search").disabled && !document.querySelector(".move-pending") && document.querySelectorAll(".card").length === 1 && visibleSessions()[0].hostId === "other"');
  await check("等待中禁用归档排序新建和再次拖动", 'byId("new-session").disabled && document.querySelector("[data-archive-all]").disabled && document.querySelector(".card .card-actions button:last-child").disabled && [...document.querySelectorAll(".column-sort")].every(button=>button.disabled) && [...document.querySelectorAll(".card")].every(card=>!card.draggable)');
  await evaluate('byId("new-session").onclick()');
  await check("等待中直接调用新建也不打开对话框", '!byId("new-target").open && !creating && saving');
  await advance(401);await until('performance.now() > suppressOpenUntil');
  await evaluate('(() => {const card=document.querySelector(".card"), event=new Event("dragstart",{cancelable:true});card.dispatchEvent(event);window.blockedDrag=event.defaultPrevented;void archiveSessions(visibleSessions(),document.querySelector("[data-archive-all]"));void changeColumnSort("to_archive",document.querySelector("#column-to_archive .column-sort"));void confirmArchiveInputs(visibleSessions()[0],document.querySelector("[data-archive-all]"));void moveCard(visibleSessions()[0],"review");void openSession(moving.id);})()');
  await check("等待中直接调用也不能重复写入", 'window.blockedDrag && saving && moving.id === data.sessions[0].id && !opening');
  assert.equal(writes.length, 1);assert.deepEqual(mutations, expectedMutations);assert.deepEqual(opens, []);
  await evaluate('document.querySelector(".card-title").click()');
  await until('!opening');
  assert.deepEqual(opens, [otherId]);cases.push("等待中允许打开另一主机的会话");
  await select("host-filter", "host:fixture");
  await evaluate('byId("search").value="";byId("search").dispatchEvent(new Event("input"))');
  await check("返回来源主机保留临时落位和来源占位", 'selectedHost === "host:fixture" && data.sessions[0].state === "in_progress" && document.querySelector(".move-pending") && document.querySelector("#column-in_progress .move-origin")');
  await select("host-filter", "all");
  refreshError = true;
  await evaluate('byId("refresh").click()');
  await until('snapshotError === "刷新断线"');
  await check("刷新失败保留健康摘要和保存等待", 'Boolean(byId("health-summary")) && byId("health-summary").textContent.includes("断开") && saving && Boolean(document.querySelector(".move-pending"))');
  refreshError = false;
  await advanceTo(1499);
  assert.equal(await evaluate('byId("move-feedback").hidden'), true);
  await advance(1);await until('!byId("move-feedback").hidden');
  await check("长等待搬运工与微光共存", 'performance.now() - moving.started === 1500 && document.querySelector(".move-pending") && byId("move-feedback").classList.contains("waiting") && !byId("move-feedback").hidden');
  await screenshot("long-wait");
  for (const width of [320, 736, 1680]) {
    await page.command("Emulation.setDeviceMetricsOverride", { width, height: 1000, deviceScaleFactor: 1, mobile: false });
    await check(`等待提示在${width}像素内`, 'document.documentElement.scrollWidth <= innerWidth && byId("move-feedback").getBoundingClientRect().left >= 0 && byId("move-feedback").getBoundingClientRect().right <= innerWidth');
    if (width === 320) await screenshot("narrow-wait");
  }
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await check("减少动画保留静态说明", 'getComputedStyle(document.querySelector(".courier")).animationName === "none" && getComputedStyle(document.querySelector(".move-pending"),"::after").animationName === "none" && document.querySelector(".move-label").textContent.includes("等待")');
  await screenshot("reduced-motion");
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await advanceTo(7999);
  assert.equal(await evaluate('Boolean(moving.slow)'), false);
  await advance(1);
  assert.equal(await evaluate('Boolean(moving.slow)'), true);
  await until('moving?.slow === true');
  await check("慢响应不假装完成", 'performance.now() - moving.started === 8000 && saving && byId("move-heading").textContent.includes("还在等确认") && !byId("feedback").textContent.includes("已保存")');
  await complete(200, true, true);
  await settled();
  await check("回执才触发完成且标题待同步独立", 'data.sessions[0].state === "review" && !document.querySelector(".move-pending") && byId("move-feedback").classList.contains("saved") && document.querySelector(".follow").textContent.includes("标题") && byId("feedback").textContent.includes("标题同步待重试")');
  heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
  await evaluate('window.staleRead');
  await check("晚到的旧快照不能回滚成功结果", 'selectedHost === "all" && data.sessions[0].state === "review" && !saving && !moving && !document.querySelector(".move-origin")');
  await screenshot("saved");
  await evaluate('for(let index=0;index<20;index++)data.sessions.push({...data.sessions[1],id:"scroll-"+index});render();byId("column-unknown").querySelector(".cards").scrollTop=180;window.previousScroll=byId("column-unknown").querySelector(".cards").scrollTop');
  await move("to_archive");
  await check("临时落位保留其他列的滚动", 'window.previousScroll > 0 && byId("column-unknown").querySelector(".cards").scrollTop === window.previousScroll');
  await advance(1050);
  await check("上一轮收尾不会打断新等待", 'saving && byId("move-feedback").hidden && moving.state === "to_archive"');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await evaluate('byId("move-dismiss").click()');
  await check("收起仅隐藏过场", 'saving && moving.dismissed && byId("move-feedback").hidden');
  await complete(409, false);
  await settled();
  await check("冲突恢复已知位置并留下核对说明", 'data.sessions[0].state === "review" && !document.querySelector(".move-pending") && document.querySelector(".move-label").textContent.includes("核对") && !byId("feedback").textContent.includes("已保存")');
  await move("unknown");
  snapshotError = true;await complete(503, true);
  await settled();await until('byId("health-summary").textContent.includes("断开")');
  await check("丢回执加读取失败不伪造回滚", 'data.sessions[0].state === "review" && unconfirmedMoves.size === 1 && byId("feedback").textContent.includes("未收到保存确认")');
  snapshotError = false;
  await evaluate('load()');
  await check("恢复读回实际位置而不重发", 'data.sessions[0].state === "unknown" && unconfirmedMoves.size === 0');
  const beforeNoop = writes.length;
  await drag(id, "#column-unknown .cards");
  await drag(id, "#health");
  assert.equal(writes.length, beforeNoop, "同列和列外不保存");cases.push("同列及列外不提交");
  await drag(id, "#column-blocked .cards");
  await check("阻塞仍弹窗提示选填原因", 'byId("editor").open && !moving && data.sessions[0].state === "unknown" && byId("note-help").textContent.includes("暂时不填")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open && editing === null');
  assert.equal(writes.length, beforeNoop, "取消不保存");
  await drag(id, "#column-blocked .cards");
  await evaluate('byId("note").value="";byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await check("空原因允许保存并保持等待保护", 'saving && moving.state === "blocked" && byId("note").readOnly && byId("save").disabled && byId("cancel").disabled && data.sessions[0].state === "unknown"');
  assert.equal(writes.length, beforeNoop + 1);assert.equal(writes.at(-1).note, "");
  await complete(409, false);await settled();
  await check("空原因失败后保留空草稿", 'byId("editor").open && !byId("note").readOnly && byId("note").value === "" && noteDraft(data.sessions[0].id) === "" && byId("feedback").textContent.includes("未收到保存确认")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open && editing === null');
  await drag(id, "#column-blocked .cards");
  await check("等待失败后的空草稿重开仍为空", 'byId("editor").open && byId("note").value === "" && noteDraft(data.sessions[0].id) === ""');
  await evaluate('byId("note").value="等待环境";byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await check("表单内等待可见且不能重复提交", 'byId("editor").contains(byId("move-feedback")) && byId("save").disabled && byId("cancel").disabled && byId("state").disabled && byId("note").readOnly && !byId("notice").textContent && data.sessions[0].state === "unknown"');
  await screenshot("blocked-form");
  await complete(409, false);await settled();
  await check("表单失败保留草稿并说明未确认", 'byId("editor").open && !byId("state").disabled && !byId("note").readOnly && byId("note").value === "等待环境" && noteDraft(data.sessions[0].id) === "等待环境" && byId("feedback").textContent.includes("未收到保存确认")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open && editing === null');
  await drag(otherId, "#column-blocked .cards");
  await check("另一会话编辑框不显示前一会话失败面板", `editing.id === ${JSON.stringify(otherId)} && byId("move-feedback").hidden && byId("move-slot").contains(byId("move-feedback")) && unconfirmedMoves.has(data.sessions[0].id) && !unconfirmedMoves.has(editing.id) && byId("note").value === editing.note`);
  await evaluate('byId("cancel").click()');await until('!byId("editor").open && editing === null');
  await drag(id, "#column-blocked .cards");
  await evaluate('byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await complete();await settled();await until('!byId("editor").open && editing === null');
  await check("表单成功才移动清草稿并保留收尾提示", 'data.sessions[0].state === "blocked" && noteDraft(data.sessions[0].id) === null && byId("move-feedback").classList.contains("saved") && !byId("move-feedback").hidden && byId("move-slot").contains(byId("move-feedback"))');
  await advance(899);
  assert.equal(await evaluate('byId("move-feedback").hidden'), false);
  await advance(1);
  assert.equal(await evaluate('byId("move-feedback").hidden'), true);
  automatic = true;await move("review");await settled();await advance(1800);
  await check("快响应不闪现搬运工", 'byId("move-feedback").hidden && !moving && !document.querySelector(".move-pending")');
  const beforeReadonly = writes.length;
  snapshot.sessions[0].readOnly = true;
  await evaluate('load()');
  assert.equal(await evaluate('data.sessions[0].readOnly'), true, "必须读回服务器只读标志");
  await evaluate('void moveCard(data.sessions[0],"unknown")');
  await check("只读卡片保持只读", '!Array.from(document.querySelectorAll(".card")).find(card => card.dataset.id === data.sessions[0].id).draggable && !saving');
  assert.equal(writes.length, beforeReadonly, "只读卡片不能发送保存请求");
  snapshot.sessions[0].readOnly = false;
  await evaluate('load()');
  assert.equal(await evaluate('data.sessions[0].readOnly'), false, "必须读回服务器解除只读");
  automatic = false;await move("unknown");
  snapshot.sessions[0].state = "in_progress";snapshot.sessions[0].version++;snapshot.sessions[0].inputVersion++;
  await complete(409, false);await settled();
  await until('data.sessions[0].state === "in_progress"');
  await check("新输入冲突保留较新的状态", 'data.sessions[0].inputVersion === 6 && data.sessions[0].state === "in_progress" && unconfirmedMoves.size === 0');
  await check("保存结束解除离开保护", '(() => {const event=new Event("beforeunload",{cancelable:true});window.dispatchEvent(event);return !event.defaultPrevented;})()');
  const beforeReload = writes.length;
  await page.command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` });
  await until('typeof data !== "undefined" && data.sessions?.[0]?.state === "in_progress"');
  await check("重载读真实位置且不重放操作", '!moving && !document.querySelector(".move-pending")');
  assert.equal(writes.length, beforeReload, "重载不能重发写入");
  assert.deepEqual(mutations, expectedMutations);
  await fs.writeFile(path.join(output, "transfer.json"), JSON.stringify({ cases, writes, opens, mutations, initialReducedMotion, browserStartup: browser.startup }, null, 2));
} catch (error) {
  await screenshot("failure");
  await fs.writeFile(path.join(output, "failure.json"), JSON.stringify({ error: error.message, cases, writes, mutations,
    initialReducedMotion, state: await evaluate('({now:performance.now(),reducedMotion:matchMedia("(prefers-reduced-motion: reduce)").matches,glowAnimation:document.querySelector(".move-pending")?getComputedStyle(document.querySelector(".move-pending"),"::after").animationName:null,hidden:document.hidden,editing,editorOpen:byId("editor").open,dragging,saving,moving,feedback:byId("move-feedback").outerHTML,ids:[...document.querySelectorAll(".card")].map(card=>({id:card.dataset.id,draggable:card.draggable,rect:card.getBoundingClientRect().toJSON()}))})') }, null, 2));
  throw error;
} finally {
  for (const { response } of responses) response.destroy();
  pendingNew?.destroy();
  const cleanup = await browser.close();
  server.closeAllConnections();await new Promise(resolve => server.close(resolve));
  await fs.writeFile(path.join(output, "cleanup.json"), JSON.stringify(cleanup));
}
console.log(JSON.stringify({ cases: cases.length, output, initialReducedMotion, browserStartup: browser.startup }));
