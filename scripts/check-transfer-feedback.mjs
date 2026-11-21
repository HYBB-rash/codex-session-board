import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";
import { inputText } from "./browser-actions.mjs";

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
    { id: 'remote:["codex","fixture","11111111-1111-4111-8111-111111111111"]', title: "远程长标题".repeat(12), state: "in_progress", remote: true, hostId: "fixture", hostLabel: "测试远端", projectId: "project" },
    { id: "local", title: "本机会话", state: "unknown", projectId: null },
    { id: 'remote:["codex","other","11111111-1111-4111-8111-111111111111"]', title: "另一主机待归档会话", state: "to_archive", remote: true, hostId: "other", hostLabel: "另一主机", projectId: "other-project", canArchive: true },
  ].map(session => ({ note: "保留原备注", reason: "", version: 3, inputVersion: 5, sectionVersion: "native:source", canArchive: false, ...session })) };
const writes = [], opens = [], mutations = [], responses = [], cases = [];
let snapshotError = false, refreshError = false, automatic = false, holdSnapshot = false, heldSnapshot, pendingNew;
const complete = async (status = 200, apply = status === 200, titlePending = false, receipt = value => value) => {
  for (let attempt = 0; attempt < 150 && !responses.length; attempt++) await delay(40);
  const { response, body } = responses.shift() || assert.fail("等待保存请求送达超时");
  const session = snapshot.sessions.find(row => row.id === body.id);
  if (apply) Object.assign(session,
    { state: body.state, note: (body.note ?? session.note).trim(), version: body.version + 1,
      sectionVersion: "native:" + body.state, titleSyncPending: titlePending });
  response.writeHead(status, { "Content-Type": "application/json" });
  response.end(JSON.stringify(status === 200 ? receipt({ ok: true, titleEnabled: true, titlePending, session }) : { error: "版本冲突或连接未确认" }));
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
const drag = async (id, selector, beforeDrop) => {
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
    if (beforeDrop) await beforeDrop();
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
const animationFrames = async (name, times) => {
  const clip = await evaluate('(() => {const rect=byId("move-feedback").getBoundingClientRect();return {x:rect.x,y:rect.y,width:rect.width,height:rect.height,scale:1};})()');
  for (const [index, time] of times.entries()) {
    await evaluate(`byId("move-dragon").getAnimations().forEach(animation=>{animation.pause();animation.currentTime=${time}})`);
    const image = await page.command("Page.captureScreenshot", { format: "png", clip });
    await fs.writeFile(path.join(output, `${name}-${index}.png`), Buffer.from(image.data, "base64"));
  }
  await evaluate('byId("move-dragon").getAnimations().forEach(animation=>{animation.currentTime=0;animation.play()})');
};
const visibility = hidden => evaluate(`window.transferHidden=${hidden};Object.defineProperty(document,"hidden",{configurable:true,get:()=>window.transferHidden});document.dispatchEvent(new Event("visibilitychange"))`);
try {
  await until('typeof transferTestClock !== "undefined" && Boolean(document.querySelector(".card"))');
  await select("host-filter", "all");
  const normalReducedMotion = await evaluate('matchMedia("(prefers-reduced-motion: reduce)").matches');
  console.error(JSON.stringify({ motion: { initialReducedMotion, normalReducedMotion } }));
  assert.equal(normalReducedMotion, false);
  assert.deepEqual(await evaluate("moveDelays"), { character: 1500, slow: 8000, success: 2000 });
  await until("moveArtReady");
  await check("龙娘图集内嵌且透明可解码", '(() => {const image=byId("move-dragon"),canvas=document.createElement("canvas");canvas.width=768;canvas.height=512;const context=canvas.getContext("2d");context.drawImage(image,0,0);return image.src.startsWith("data:image/webp;base64,") && image.naturalWidth===768 && image.naturalHeight===512 && context.getImageData(0,0,1,1).data[3]===0 && context.getImageData(96,128,1,1).data[3]>0;})()');
  const id = snapshot.sessions[0].id, otherId = snapshot.sessions[2].id;
  await evaluate('window.pendingCreation=createSession("project");void 0');
  await until('creating');
  await check("新建等待中禁用拖动归档排序与筛选", 'byId("new-session").disabled && byId("host-filter").disabled && document.querySelector("[data-archive-all]").disabled && [...document.querySelectorAll(".card")].every(card=>!card.draggable) && [...document.querySelectorAll(".column-sort")].every(button=>button.disabled)');
  await evaluate('(() => {const card=document.querySelector(".card"), event=new Event("dragstart",{cancelable:true});card.dispatchEvent(event);window.creationBlockedDrag=event.defaultPrevented;void moveCard(data.sessions[0],"review");void archiveSessions([data.sessions[2]],document.querySelector("[data-archive-all]"));void confirmArchiveInputs(data.sessions[2],document.querySelector("[data-archive-all]"));void changeColumnSort("to_archive",document.querySelector("#column-to_archive .column-sort"));byId("draft-restart").onclick();openEditor(data.sessions[0].id,"blocked");byId("edit-form").requestSubmit();})()');
  await check("新建先开始时各写入口也保持互斥", 'creating && !saving && !document.querySelector(".drag-source") && window.creationBlockedDrag && !byId("editor").open');
  for (let attempt = 0; attempt < 150 && !pendingNew; attempt++) await delay(40);
  assert.equal(writes.length, 0);assert.equal(mutations.length, 1);
  assert.equal(mutations[0].path, "/api/new");assert.equal(mutations[0].body.projectId, "project");
  assert.equal(Object.hasOwn(mutations[0].body,"requestId"), false);
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
  assert.deepEqual(writes[0], { id, state: "review", version: 3, inputVersion: 5, sectionVersion: "native:source" });
  await select("host-filter", "host:other");
  await select("project-filter", "project:other-project");
  await check("等待中主机和项目筛选仍可使用", 'selectedHost === "host:other" && selectedProject === "project:other-project" && !byId("host-filter").disabled && !byId("project-filter").disabled && !document.querySelector(".move-pending,.move-origin") && visibleSessions().length === 1');
  await evaluate('byId("filter-toggle").click();byId("search").value="另一主机";byId("search").dispatchEvent(new Event("input"))');
  await check("等待中搜索保持所选主机范围", 'filterMode === "search" && !byId("search").disabled && !document.querySelector(".move-pending,.move-origin") && document.querySelectorAll(".card").length === 1 && visibleSessions()[0].hostId === "other"');
  await check("等待中禁用归档排序新建和再次拖动", 'byId("new-session").disabled && document.querySelector("[data-archive-all]").disabled && document.querySelector(".card .card-actions button:last-child").disabled && [...document.querySelectorAll(".column-sort")].every(button=>button.disabled) && [...document.querySelectorAll(".card")].every(card=>!card.draggable)');
  await evaluate('byId("new-session").onclick()');
  await check("等待中直接调用新建也不打开对话框", '!byId("new-target").open && !creating && saving');
  await advance(401);
  await evaluate('(() => {const card=document.querySelector(".card"), event=new Event("dragstart",{cancelable:true});card.dispatchEvent(event);window.blockedDrag=event.defaultPrevented;void archiveSessions(visibleSessions(),document.querySelector("[data-archive-all]"));void changeColumnSort("to_archive",document.querySelector("#column-to_archive .column-sort"));void confirmArchiveInputs(visibleSessions()[0],document.querySelector("[data-archive-all]"));void moveCard(visibleSessions()[0],"review");void openSession(moving.id);})()');
  await check("等待中直接调用也不能重复写入", 'window.blockedDrag && saving && moving.id === data.sessions[0].id && !opening');
  assert.equal(writes.length, 1);assert.deepEqual(mutations, expectedMutations);assert.deepEqual(opens, []);
  await evaluate('document.querySelector(".card-title").click();byId("reply-open").click()');
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
  await check("长等待龙娘与微光共存且此刻才起算可见时间", 'performance.now() - moving.started === 1500 && moving.shownAt === performance.now() && document.querySelector(".move-pending") && byId("move-feedback").classList.contains("waiting") && !byId("move-feedback").hidden');
  await check("等待态播放眨眼摆尾帧", 'getComputedStyle(byId("move-dragon")).animationName === "dragon-wait" && getComputedStyle(byId("move-dragon")).top === "0px"');
  await screenshot("long-wait");
  await animationFrames("waiting-animation", [0, 1560, 1800, 2040, 2280, 2399]);
  for (const [width, height] of [[320, 568], [320, 1000], [736, 1000], [1680, 1000]]) {
    await page.command("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
    await evaluate(`document.body.style.minHeight=${JSON.stringify(height === 568 ? "calc(100vh + 32px)" : "")}`);
    await check(`等待提示在${width}乘${height}像素内`, 'document.documentElement.scrollWidth <= innerWidth && byId("move-feedback").getBoundingClientRect().left >= 0 && byId("move-feedback").getBoundingClientRect().right <= innerWidth && byId("move-feedback").getBoundingClientRect().top >= 0 && byId("move-feedback").getBoundingClientRect().bottom <= innerHeight');
    if (width === 320) await screenshot(height === 568 ? "narrow-portrait-wait" : "narrow-wait");
  }
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await check("减少动画保留静态龙娘和说明", 'getComputedStyle(byId("move-dragon")).animationName === "none" && !byId("move-dragon").hidden && getComputedStyle(document.querySelector(".move-pending"),"::after").animationName === "none" && document.querySelector(".move-label").textContent.includes("等待")');
  await screenshot("reduced-motion");
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await advanceTo(7999);
  assert.equal(await evaluate('Boolean(moving.slow)'), false);
  await advance(1);
  assert.equal(await evaluate('Boolean(moving.slow)'), true);
  await until('moving?.slow === true');
  await check("慢响应不假装完成", 'performance.now() - moving.started === 8000 && saving && byId("move-heading").textContent.includes("还在等确认") && !byId("feedback").textContent.includes("已保存")');
  await check("超过可见两秒仍未确认时继续展示", 'performance.now() - moving.shownAt === 6500 && !byId("move-feedback").hidden && byId("move-feedback").classList.contains("waiting")');
  const staleSnapshot = heldSnapshot;
  heldSnapshot = undefined;holdSnapshot = true;
  const otherBeforeReceipt = structuredClone(snapshot.sessions[2]);
  await select("host-filter", "host:other");
  await complete(200, true, true);
  await settled();
  const receiptNotice = await evaluate('byId("feedback").textContent');
  assert.match(receiptNotice, /状态已保存.*标题同步待重试/);
  assert.deepEqual(await evaluate(`data.sessions.find(row=>row.id===${JSON.stringify(otherId)})`), otherBeforeReceipt);
  await check("A迟到回执不改变当前B主机同UUID卡片", `selectedHost === "host:other" && visibleSessions().length === 1 && visibleSessions()[0].id === ${JSON.stringify(otherId)} && visibleSessions()[0].state === "to_archive"`);
  await select("host-filter", "all");
  staleSnapshot.response.setHeader("Content-Type", "application/json");staleSnapshot.response.end(staleSnapshot.text);
  await evaluate('window.staleRead');
  for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
  assert(heldSnapshot);
  await check("刷新尚未返回也立即使用回执的原生位置", 'data.sessions[0].sectionVersion === "native:review" && !saving');
  await check("回执才触发完成且标题待同步独立", 'data.sessions[0].state === "review" && !document.querySelector(".move-pending") && byId("move-feedback").classList.contains("saved") && document.querySelector(".follow").textContent.includes("标题")');
  await check("晚到的旧快照不能回滚成功结果", 'selectedHost === "all" && data.sessions[0].state === "review" && !saving && !moving && !document.querySelector(".move-origin")');
  await screenshot("saved");
  await check("成功态切到庆祝帧", 'getComputedStyle(byId("move-dragon")).animationName === "dragon-success" && getComputedStyle(byId("move-dragon")).top === "-144px"');
  await animationFrames("success-animation", [0, 300, 600, 900, 1500, 1999]);
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await check("成功态减少动画保留静态勾选", 'getComputedStyle(byId("move-dragon")).animationName === "none" && getComputedStyle(byId("move-dragon")).transform.includes("-324") && byId("move-feedback").classList.contains("saved")');
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  await evaluate('for(let index=0;index<20;index++)data.sessions.push({...data.sessions[1],id:"scroll-"+index});render();byId("column-unknown").querySelector(".cards").scrollTop=180;window.previousScroll=byId("column-unknown").querySelector(".cards").scrollTop');
  await move("to_archive");
  for (let attempt = 0; attempt < 100 && writes.length < 2; attempt++) await delay(20);
  assert.equal(writes[1].sectionVersion, "native:review");
  heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
  await check("临时落位保留其他列的滚动", 'window.previousScroll > 0 && byId("column-unknown").querySelector(".cards").scrollTop === window.previousScroll');
  await advance(1050);
  await check("上一轮收尾不会打断新等待", 'saving && byId("move-feedback").hidden && moving.state === "to_archive"');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await advance(501);
  await check("跨过上一轮两秒截止点不隐藏新龙娘", 'saving && performance.now()-moving.started === 2001 && !byId("move-feedback").hidden && byId("move-feedback").classList.contains("waiting")');
  await evaluate('byId("move-dismiss").click()');
  await check("收起仅隐藏过场", 'saving && moving.dismissed && byId("move-feedback").hidden');
  await advanceTo(8500);
  await check("手动收起后慢等待计时器也不重新拉起", 'saving && moving.dismissed && byId("move-feedback").hidden && moveFeedback === null');
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
  for (const malformed of ["missing", "wrong-host", "wrong-id"]) {
    const beforeUnknown = writes.length;
    holdSnapshot = true;heldSnapshot = undefined;
    await move("unknown");
    await complete(200, true, false, result => {
      if (malformed === "missing") delete result.session;
      else if (malformed === "wrong-host") result.session = {...result.session,hostId:"other"};
      else result.session = {...result.session,id:'remote:["codex","fixture","22222222-2222-4222-8222-222222222222"]'};
      return result;
    });
    await settled();
    for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
    assert(heldSnapshot, "未知回执必须读取快照确认");
    await check(`${malformed}回执保持待核对且不伪报成功`, 'unconfirmedMoves.size === 1 && byId("feedback").textContent.includes("未收到保存确认") && !byId("feedback").textContent.includes("已保存")');
    heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
    await until("unconfirmedMoves.size === 0");
    assert.equal(writes.length, beforeUnknown + 1, "确认读取不能重发保存");
  }
  const beforeNoop = writes.length;
  await drag(id, "#column-unknown .cards");
  await drag(id, "#health");
  assert.equal(writes.length, beforeNoop, "同列和列外不保存");cases.push("同列及列外不提交");
  await drag(id, "#column-blocked .cards");
  await check("阻塞仍弹窗提示选填原因", 'byId("editor").open && !moving && data.sessions[0].state === "unknown" && byId("note-help").textContent.includes("暂时不填")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open');
  assert.equal(writes.length, beforeNoop, "取消不保存");
  await drag(id, "#column-blocked .cards");
  await inputText(page, "#note", "");await evaluate('byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await check("空原因允许保存并保持等待保护", 'saving && moving.state === "blocked" && byId("note").readOnly && byId("save").disabled && byId("cancel").disabled && data.sessions[0].state === "unknown"');
  assert.equal(writes.length, beforeNoop + 1);assert.equal(writes.at(-1).note, "");
  await complete(409, false);await settled();
  await check("空原因失败后保留空草稿", 'byId("editor").open && !byId("note").readOnly && byId("note").value === "" && byId("notice").textContent.includes("草稿已保留") && byId("feedback").textContent.includes("未收到保存确认")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open');
  await drag(id, "#column-blocked .cards");
  await check("等待失败后的空草稿重开仍为空", 'byId("editor").open && byId("note").value === "" && byId("notice").textContent.includes("已恢复")');
  await inputText(page, "#note", "等待环境");await evaluate('byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await check("表单内等待可见且不能重复提交", 'byId("editor").contains(byId("move-feedback")) && byId("save").disabled && byId("cancel").disabled && byId("state").disabled && byId("note").readOnly && !byId("notice").textContent && data.sessions[0].state === "unknown"');
  await screenshot("blocked-form");
  await complete(409, false);await settled();
  await check("表单失败保留草稿并说明未确认", 'byId("editor").open && !byId("state").disabled && !byId("note").readOnly && byId("note").value === "等待环境" && byId("notice").textContent.includes("草稿已保留") && byId("feedback").textContent.includes("未收到保存确认")');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open');
  await drag(otherId, "#column-blocked .cards");
  await check("另一会话编辑框不显示前一会话失败面板", `byId("dialog-title").textContent.startsWith(data.sessions[2].title) && byId("move-feedback").hidden && byId("move-slot").contains(byId("move-feedback")) && unconfirmedMoves.has(data.sessions[0].id) && !unconfirmedMoves.has(${JSON.stringify(otherId)}) && byId("note").value === data.sessions[2].note`);
  await evaluate('byId("cancel").click()');await until('!byId("editor").open');
  await drag(id, "#column-blocked .cards");
  await evaluate('byId("edit-form").requestSubmit()');
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await complete();await settled();await until('!byId("editor").open');
  await check("表单成功才移动并保留收尾提示", 'data.sessions[0].state === "blocked" && data.sessions[0].note === "等待环境" && byId("move-feedback").classList.contains("saved") && !byId("move-feedback").hidden && byId("move-slot").contains(byId("move-feedback"))');
  await advance(1999);
  assert.equal(await evaluate('byId("move-feedback").hidden'), false);
  await advance(1);
  assert.equal(await evaluate('byId("move-feedback").hidden'), true);
  automatic = true;await move("review");await settled();await advance(1800);
  await check("快响应不闪现龙娘", 'byId("move-feedback").hidden && !moving && !document.querySelector(".move-pending")');
  automatic = false;
  await drag(id, "#column-unknown .cards");
  await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await advance(1);await complete();await settled();
  await check("刚可见即确认不延迟保存或后续拖动", '!saving && !moving && data.sessions[0].state === "unknown" && [...document.querySelectorAll(".card")].every(card=>card.draggable) && moveFeedback.result === "saved" && performance.now()-moveFeedback.shownAt === 1');
  await advance(1999);
  await check("成功画面从确认可见起完整保留1999毫秒", '!byId("move-feedback").hidden && performance.now()-moveFeedback.visibleSince === 1999 && performance.now()-moveFeedback.shownAt === 2000');
  await advance(1);
  await check("成功画面到两秒后收起", 'byId("move-feedback").hidden && moveFeedback === null && !saving');
  await move("review");await advanceTo(1500);await complete();await settled();
  await evaluate('byId("move-dismiss").click()');
  await check("成功态也可立即收起", 'byId("move-feedback").hidden && moveFeedback === null');
  await drag(id, "#column-unknown .cards");await advanceTo(2001);
  await check("成功态收起后的新拖动不被旧回调隐藏", 'saving && !byId("move-feedback").hidden && moveFeedback === moving');
  await complete();await settled();
  await move("review");await advanceTo(1500);await complete(409, false);await settled();
  await advance(5000);
  await check("失败态持续显示真实未确认状态且没有成功动画", '!byId("move-feedback").hidden && byId("move-feedback").classList.contains("unconfirmed") && byId("move-detail").textContent.includes("未收到保存确认") && getComputedStyle(byId("move-dragon")).top === "0px" && getComputedStyle(byId("move-dragon")).animationName === "none"');
  await evaluate('byId("move-dismiss").click()');
  await visibility(true);
  await move("review");await advanceTo(1500);
  await check("后台等待不提前记录可见时间", 'byId("move-feedback").hidden && !moving.shown && moving.shownAt === undefined');
  await complete();await settled();await advance(10000);
  await visibility(false);await until('!byId("move-feedback").hidden');
  await advance(1999);
  await check("后台完成回到前台才起算成功两秒", 'moveFeedback.result === "saved" && performance.now()-moveFeedback.shownAt === 1999 && !byId("move-feedback").hidden');
  await advance(1);
  await move("unknown");await advanceTo(1500);await complete();await settled();
  await advance(750);await visibility(true);await advance(10000);await visibility(false);
  await advance(1249);
  await check("成功画面暂停后台耗时并续足观看时间", '!byId("move-feedback").hidden && moveFeedback.remaining === 1250');
  await advance(1);
  await check("成功画面前台累计两秒收起", 'byId("move-feedback").hidden && moveFeedback === null');
  await evaluate('delete document.hidden');
  holdSnapshot = true;heldSnapshot = undefined;
  await move("review");
  await complete(200, true, false, result => {
    Object.assign(result.session, {version:result.session.version+7,inputVersion:9,
      state:"blocked",followUp:true,reason:"保存后出现新的原生状态和用户输入"});
    return result;
  });
  await settled();
  for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
  assert(heldSnapshot);
  assert.deepEqual(await evaluate("data.sessions[0]"), snapshot.sessions[0]);
  await check("刷新前完整采用确认版本列及新输入提示", 'data.sessions[0].state === "blocked" && data.sessions[0].inputVersion === 9 && data.sessions[0].followUp === true && moving === null');
  heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
  await evaluate("load()");
  snapshot.fourColumns = true;
  await evaluate("load()");
  holdSnapshot = true;heldSnapshot = undefined;
  await move("review");await advanceTo(1500);await until('!byId("move-feedback").hidden');
  await complete(200, true, false, result => {
    Object.assign(result.session, {state:null,sectionVersion:"unsectioned",placementReason:"原生侧栏默认区"});
    return result;
  });
  await settled();
  for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
  assert(heldSnapshot);
  assert.deepEqual(await evaluate("data.sessions[0]"), snapshot.sessions[0]);
  await check("未归列确认保留实际位置并显示可读完成提示", 'data.sessions[0].state === null && data.sessions[0].placementReason === "原生侧栏默认区" && document.querySelector(".placement-reason").textContent === "原生侧栏默认区" && byId("move-detail").textContent.includes("未归列") && !byId("move-detail").textContent.includes("undefined")');
  heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
  await evaluate("load()");
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
  automatic = false;
  const beforeDrag = {...snapshot.sessions[0]};
  await evaluate('byId("unplaced").open=true');
  await drag(id,"#column-review .cards",async () => {
    Object.assign(snapshot.sessions[0],{state:"in_progress",version:beforeDrag.version+1,inputVersion:beforeDrag.inputVersion+1});
    await evaluate("load()");
  });
  assert.deepEqual(writes.at(-1),{id,state:"review",version:beforeDrag.version,
    inputVersion:beforeDrag.inputVersion,sectionVersion:beforeDrag.sectionVersion});
  await complete(409, false);await settled();
  await until('data.sessions[0].state === "in_progress"');
  await check("新输入冲突保留较新的状态", `data.sessions[0].inputVersion === 10 && data.sessions[0].state === "in_progress" &&
    data.sessions[0].note === ${JSON.stringify(beforeDrag.note)} && unconfirmedMoves.size === 0 &&
    !document.querySelector(".move-pending,.move-origin,.drop-target,.drag-source") &&
    [...document.querySelectorAll(".column")].every(node=>Number(node.querySelector(".column-count").textContent)===node.querySelectorAll(".card").length)`);
  await check("保存结束解除离开保护", '(() => {const event=new Event("beforeunload",{cancelable:true});window.dispatchEvent(event);return !event.defaultPrevented;})()');
  const beforeReload = writes.length;
  await page.command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` });
  await until('typeof data !== "undefined" && data.sessions?.[0]?.state === "in_progress"');
  await check("重载读真实位置且不重放操作", '!moving && !document.querySelector(".move-pending")');
  assert.equal(writes.length, beforeReload, "重载不能重发写入");
  assert.deepEqual(mutations, expectedMutations);
  await page.command("Page.addScriptToEvaluateOnNewDocument", { source: 'const originalDragonDecode=HTMLImageElement.prototype.decode;HTMLImageElement.prototype.decode=function(){if(this.id!=="move-dragon")return originalDragonDecode.call(this);return new Promise((resolve,reject)=>{window.releaseDragonArt=()=>originalDragonDecode.call(this).then(resolve,reject);});};' });
  await page.command("Page.navigate", { url: `http://127.0.0.1:${server.address().port}/` });
  await until('typeof releaseDragonArt === "function" && data.sessions?.length > 0');
  await move("review");await advanceTo(1500);
  await check("素材解码前不把空白计为角色可见", '!moveArtReady && !moving.shown && byId("move-feedback").hidden');
  await complete();await settled();await advance(10000);
  await evaluate('releaseDragonArt()');await until('!byId("move-feedback").hidden');
  await advance(1999);
  await check("慢解码完成后成功画面仍有完整两秒", 'moveArtReady && moveFeedback.result === "saved" && performance.now()-moveFeedback.shownAt === 1999 && !byId("move-feedback").hidden');
  await advance(1);
  await check("慢解码收尾不阻止或重放保存", 'moveFeedback === null && !saving && byId("move-feedback").hidden');
  await fs.writeFile(path.join(output, "transfer.json"), JSON.stringify({ cases, writes, opens, mutations, initialReducedMotion, browserStartup: browser.startup }, null, 2));
} catch (error) {
  await screenshot("failure");
  await fs.writeFile(path.join(output, "failure.json"), JSON.stringify({ error: error.message, cases, writes, mutations,
    initialReducedMotion, state: await evaluate('({now:performance.now(),reducedMotion:matchMedia("(prefers-reduced-motion: reduce)").matches,glowAnimation:document.querySelector(".move-pending")?getComputedStyle(document.querySelector(".move-pending"),"::after").animationName:null,hidden:document.hidden,editorOpen:byId("editor").open,dragging:board.drag.source,saving,moving,sessions:data.sessions,feedbackText:byId("feedback").textContent,follow:[...document.querySelectorAll(".follow")].map(node=>node.textContent),selectedHost,selectedProject,feedback:byId("move-feedback").outerHTML,ids:[...document.querySelectorAll(".card")].map(card=>({id:card.dataset.id,draggable:card.draggable,rect:card.getBoundingClientRect().toJSON()}))})') }, null, 2));
  throw error;
} finally {
  for (const { response } of responses) response.destroy();
  pendingNew?.destroy();
  const cleanup = await browser.close();
  server.closeAllConnections();await new Promise(resolve => server.close(resolve));
  await fs.writeFile(path.join(output, "cleanup.json"), JSON.stringify(cleanup));
}
console.log(JSON.stringify({ cases: cases.length, output, initialReducedMotion, browserStartup: browser.startup }));
