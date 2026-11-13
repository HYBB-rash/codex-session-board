import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

const output = process.argv[2] || await fs.mkdtemp(path.join(os.tmpdir(), "board-transfer-"));
await fs.mkdir(output, { recursive: true });
const html = await fs.readFile(new URL("../index.html", import.meta.url), "utf8");
const snapshot = { states: { unknown: "待判断", in_progress: "进行中", review: "待审阅", blocked: "已阻塞", to_archive: "待归档" },
  health: {}, projectCatalog: { loaded: true, items: [{ id: "project", label: "测试项目", paths: ["/fixture"] }] },
  sessions: [
    { id: 'remote:["codex","fixture","thread"]', title: "远程长标题".repeat(12), state: "in_progress", remote: true, hostLabel: "测试远端", projectId: "project" },
    { id: "local", title: "本机会话", state: "unknown", projectId: null },
  ].map(session => ({ ...session, note: "保留原备注", version: 3, inputVersion: 5, sectionVersion: "native:source", canArchive: false })) };
const writes = [], opens = [], responses = [], cases = [];
let snapshotError = false, automatic = false, holdSnapshot = false, heldSnapshot;
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
  response.setHeader("Content-Type", "application/json");response.end('{"ok":true}');
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const browser = await launchBrowser();
const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
const evaluate = expression => page.evaluate(expression);
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
  await until('Boolean(document.querySelector(".card"))');
  const id = snapshot.sessions[0].id;
  holdSnapshot = true;
  await evaluate('window.staleRead=load();void 0');
  for (let attempt = 0; attempt < 100 && !heldSnapshot; attempt++) await delay(20);
  assert(heldSnapshot, "必须实际挂起旧快照请求");
  await drag(id, "#column-review .cards");
  await check("临时落位不修改已确认记录", 'saving && data.sessions[0].state === "in_progress" && document.querySelector(".move-pending").closest(".column").dataset.state === "review" && byId("column-review").querySelector(".column-count").textContent === "0 + 1"');
  await delay(300);
  await check("保存中触发离开保护", '(() => {const event=new Event("beforeunload",{cancelable:true});window.dispatchEvent(event);return event.defaultPrevented;})()');
  await check("短等待仅有微光", 'getComputedStyle(document.querySelector(".move-pending"),"::after").opacity !== "0" && byId("move-feedback").hidden');
  await screenshot("short-wait");
  await evaluate('void moveCard(data.sessions[0],"to_archive");void load();byId("refresh").click()');
  assert.equal(writes.length, 1, "重复操作不能追加写入");
  assert.deepEqual(writes[0], { id, state: "review", note: "保留原备注", version: 3, inputVersion: 5, sectionVersion: "native:source" });
  await evaluate('byId("filter-toggle").click();byId("search").value="本机";byId("search").dispatchEvent(new Event("input"))');
  await check("等待中搜索仍可使用", 'filterMode === "search" && !document.querySelector(".move-pending") && document.querySelectorAll(".card").length === 1');
  await until('!byId("move-feedback").hidden');
  await evaluate('void openSession("local")');
  await until('!opening');
  assert.deepEqual(opens, ["local"], "等待中允许打开其他会话");
  await evaluate('byId("search").value="";byId("search").dispatchEvent(new Event("input"))');
  await check("长等待搬运工与微光共存", 'document.querySelector(".move-pending") && byId("move-feedback").classList.contains("waiting") && !byId("move-feedback").hidden');
  await screenshot("long-wait");
  for (const width of [320, 736, 1680]) {
    await page.command("Emulation.setDeviceMetricsOverride", { width, height: 1000, deviceScaleFactor: 1, mobile: false });
    await check(`等待提示在${width}像素内`, 'document.documentElement.scrollWidth <= innerWidth && byId("move-feedback").getBoundingClientRect().left >= 0 && byId("move-feedback").getBoundingClientRect().right <= innerWidth');
    if (width === 320) await screenshot("narrow-wait");
  }
  await page.command("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await check("减少动画保留静态说明", 'getComputedStyle(document.querySelector(".courier")).animationName === "none" && getComputedStyle(document.querySelector(".move-pending"),"::after").animationName === "none" && document.querySelector(".move-label").textContent.includes("等待")');
  await screenshot("reduced-motion");
  await page.command("Emulation.setEmulatedMedia", { features: [] });
  await until('moving?.slow === true');
  await check("慢响应不假装完成", 'saving && byId("move-heading").textContent.includes("还在等确认") && !byId("feedback").textContent.includes("已保存")');
  await complete(200, true, true);
  await settled();
  await check("回执才触发完成且标题待同步独立", 'data.sessions[0].state === "review" && !document.querySelector(".move-pending") && byId("move-feedback").classList.contains("saved") && document.querySelector(".follow").textContent.includes("标题") && byId("feedback").textContent.includes("标题同步待重试")');
  heldSnapshot.response.setHeader("Content-Type", "application/json");heldSnapshot.response.end(heldSnapshot.text);
  await evaluate('window.staleRead');
  await check("晚到的旧快照不能回滚成功结果", 'data.sessions[0].state === "review"');
  await screenshot("saved");
  await evaluate('for(let index=0;index<20;index++)data.sessions.push({...data.sessions[1],id:"scroll-"+index});render();byId("column-unknown").querySelector(".cards").scrollTop=180;window.previousScroll=byId("column-unknown").querySelector(".cards").scrollTop');
  await move("to_archive");
  await check("临时落位保留其他列的滚动", 'window.previousScroll > 0 && byId("column-unknown").querySelector(".cards").scrollTop === window.previousScroll');
  await delay(1050);
  await check("上一轮收尾不会打断新等待", 'saving && byId("move-feedback").hidden && moving.state === "to_archive"');
  await until('!byId("move-feedback").hidden');
  await evaluate('byId("move-dismiss").click()');
  await check("收起仅隐藏过场", 'saving && moving.dismissed && byId("move-feedback").hidden');
  await complete(409, false);
  await settled();
  await check("冲突恢复已知位置并留下核对说明", 'data.sessions[0].state === "review" && !document.querySelector(".move-pending") && document.querySelector(".move-label").textContent.includes("核对") && !byId("feedback").textContent.includes("已保存")');
  await move("unknown");
  snapshotError = true;await complete(503, true);
  await settled();await until('byId("health").textContent.includes("断线")');
  await check("丢回执加读取失败不伪造回滚", 'data.sessions[0].state === "review" && unconfirmedMoves.size === 1 && byId("feedback").textContent.includes("未收到保存确认")');
  snapshotError = false;
  await evaluate('load()');
  await check("恢复读回实际位置而不重发", 'data.sessions[0].state === "unknown" && unconfirmedMoves.size === 0');
  const beforeNoop = writes.length;
  await drag(id, "#column-unknown .cards");
  await drag(id, "#health");
  assert.equal(writes.length, beforeNoop, "同列和列外不保存");cases.push("同列及列外不提交");
  await drag(id, "#column-blocked .cards");
  await check("阻塞先填写原因", 'byId("editor").open && !moving && data.sessions[0].state === "unknown"');
  await evaluate('byId("cancel").click()');await until('!byId("editor").open');
  assert.equal(writes.length, beforeNoop, "取消不保存");
  await drag(id, "#column-blocked .cards");
  await evaluate('byId("note").value="   ";byId("edit-form").requestSubmit()');
  assert.equal(writes.length, beforeNoop, "空原因不提交");
  await evaluate('byId("note").value="等待环境";byId("edit-form").requestSubmit()');
  await until('!byId("move-feedback").hidden');
  await check("表单内等待可见且不能重复提交", 'byId("editor").contains(byId("move-feedback")) && byId("save").disabled && byId("cancel").disabled && byId("state").disabled && byId("note").readOnly && !byId("notice").textContent && data.sessions[0].state === "unknown"');
  await screenshot("blocked-form");
  await complete(409, false);await settled();
  await check("表单失败保留草稿并说明未确认", 'byId("editor").open && !byId("state").disabled && !byId("note").readOnly && byId("note").value === "等待环境" && noteDraft(data.sessions[0].id) === "等待环境" && byId("feedback").textContent.includes("未收到保存确认")');
  await evaluate('byId("edit-form").requestSubmit()');
  await complete();await settled();await until('!byId("editor").open');
  await check("表单成功才移动且清除草稿", 'data.sessions[0].state === "blocked" && noteDraft(data.sessions[0].id) === null');
  automatic = true;await move("review");await settled();await delay(1800);
  await check("快响应不闪现搬运工", 'byId("move-feedback").hidden && !moving && !document.querySelector(".move-pending")');
  await evaluate('data.sessions[0].readOnly=true;render();void moveCard(data.sessions[0],"unknown")');
  await check("只读卡片保持只读", '!Array.from(document.querySelectorAll(".card")).find(card => card.dataset.id === data.sessions[0].id).draggable && !saving');
  await evaluate('data.sessions[0].readOnly=false;render()');
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
  await fs.writeFile(path.join(output, "transfer.json"), JSON.stringify({ cases, writes, opens }, null, 2));
} catch (error) {
  await screenshot("failure");
  await fs.writeFile(path.join(output, "failure.json"), JSON.stringify({ error: error.message, cases, writes,
    state: await evaluate('({dragging,saving,moving,ids:[...document.querySelectorAll(".card")].map(card=>({id:card.dataset.id,draggable:card.draggable,rect:card.getBoundingClientRect().toJSON()}))})') }, null, 2));
  throw error;
} finally {
  for (const { response } of responses) response.destroy();
  const cleanup = await browser.close();
  server.closeAllConnections();await new Promise(resolve => server.close(resolve));
  await fs.writeFile(path.join(output, "cleanup.json"), JSON.stringify(cleanup));
}
console.log(JSON.stringify({ cases: cases.length, output }));
