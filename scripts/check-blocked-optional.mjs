import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

const source = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.join(os.homedir(), ".csb-dev", createHash("sha256").update(source).digest("hex").slice(0, 12));
const output = path.resolve(process.argv[2] || path.join(root, "blocked-optional.json"));
assert(!output.startsWith(source + path.sep), "报告必须放在源码之外");
const runtime = JSON.parse(await fs.readFile(path.join(root, "runtime.json"), "utf8"));
assert.equal(runtime.phase, "running", "请先启动本工作树的隔离开发环境");
assert.equal(await fs.realpath(root), root);
const response = await fetch(new URL("/api/info", runtime.url), { redirect: "error" });
const info = await response.json();
assert(response.ok && info.pid === runtime.service_pid && info.build === runtime.program_sha256 &&
  info.dataDir === path.join(root, "data"), "服务身份不属于本工作树，拒绝写入");
const report = { passed: false, source, source_sha: runtime.source_sha, fingerprint: runtime.fingerprint, runtime: info, cases: [] };
const browser = await launchBrowser();
let page;
const until = async (operation, message) => {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await operation()) return;
    await delay(50);
  }
  throw new Error(message);
};
const evaluate = expression => page.evaluate(expression);
const snapshot = () => evaluate('(async()=>{const response=await fetch("/api/sessions");if(!response.ok)throw Error(response.status);return response.json()})()');
const current = async () => {
  const view = await snapshot();
  const row = view.sessions.find(row => row.id === report.id);
  assert(row, "测试会话消失");
  return row;
};
const fields = row => [row.state, row.note, row.version, row.inputVersion, row.sectionVersion, row.manual];
const point = selector => evaluate(`(()=>{
  const node=document.querySelector(${JSON.stringify(selector)});if(!node)throw Error("元素未找到");
  node.scrollIntoView({block:"center",inline:"nearest",behavior:"instant"});
  const bounds=node.getBoundingClientRect();return {x:bounds.left+bounds.width/2,y:bounds.top+Math.min(bounds.height/2,40)};
})()`);
const click = async selector => {
  const position = await point(selector);
  await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...position, button: "left", buttons: 1, clickCount: 1 });
  await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...position, button: "left", clickCount: 1 });
};
const drag = async state => {
  await page.command("Input.setInterceptDrags", { enabled: true });
  try {
    const start = await point(`.card[data-id="${report.id}"] .card-title`);
    const end = await point(`#column-${state} .cards`);
    const intercepted = page.event("Input.dragIntercepted");
    intercepted.catch(() => {});
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", ...start });
    await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...start, button: "left", buttons: 1, clickCount: 1 });
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", x: start.x + 30, y: start.y + 15, buttons: 1, button: "left" });
    const { data } = await intercepted;
    assert(data.items.some(item => item.mimeType === "text/plain" && item.data === report.id));
    for (const type of ["dragEnter", "dragOver", "drop"])
      await page.command("Input.dispatchDragEvent", { type, ...end, data });
    await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...end, button: "left", clickCount: 1 });
  } finally { await page.command("Input.setInterceptDrags", { enabled: false }); }
};
const open = async () => {
  await drag("blocked");
  await until(() => evaluate('document.querySelector("#editor").open'), "拖入阻塞列未打开弹窗");
  assert.equal(await evaluate('document.activeElement.id'), "note");
  assert.match(await evaluate('document.querySelector("#note-help").textContent'), /暂时不填也可以保存/);
};
const input = async text => {
  await evaluate('document.querySelector("#note").focus();document.querySelector("#note").select()');
  if (text) await page.command("Input.insertText", { text });
  else {
    await page.command("Input.dispatchKeyEvent", { type: "keyDown", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 });
    await page.command("Input.dispatchKeyEvent", { type: "keyUp", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 });
  }
};
const escape = async () => {
  for (const type of ["keyDown", "keyUp"])
    await page.command("Input.dispatchKeyEvent", { type, key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
};
const closed = () => until(() => evaluate('!document.querySelector("#editor").open && !saving'), "弹窗未关闭");
const save = async () => { await click("#save"); await closed(); await evaluate("load()"); };
const check = async (name, operation) => { await operation(); report.cases.push({ name, passed: true }); };
try {
  page = await browser.page(runtime.url);
  await until(() => evaluate('typeof data!=="undefined" && data?.health.runtimeVerified && data?.health.nativeVerified && document.querySelector(".card")'),
    "隔离看板未就绪");
  const view = await snapshot();
  report.id = view.sessions.find(row => !row.remote && !row.readOnly)?.id;
  assert(/^[0-9a-f-]{36}$/.test(report.id || ""), "没有可写的隔离会话");
  await evaluate(`window.optionalStatePosts=0;window.optionalFetch=window.fetch;
    window.fetch=async function(url,options){if(String(url)==="/api/state")optionalStatePosts++;return optionalFetch.call(this,url,options)};`);
  await check("取消按钮不改状态或备注", async () => {
    const before = fields(await current());
    await open(); await input("不应保存的内容"); await click("#cancel"); await closed();
    assert.deepEqual(fields(await current()), before);
    assert.equal(await evaluate("optionalStatePosts"), 0);
  });
  await check("Escape取消不写入", async () => {
    const before = fields(await current());
    await open(); await input(""); await escape(); await closed();
    assert.deepEqual(fields(await current()), before);
    assert.equal(await evaluate("optionalStatePosts"), 0);
  });
  await check("空原因保存为人工阻塞", async () => {
    await open(); await input(""); await save();
    const saved = await current();
    assert.equal(saved.state, "blocked"); assert.equal(saved.note, ""); assert.equal(saved.manual, true);
  });
  await check("阻塞同列弹窗可补原因且打开不写入", async () => {
    const before = fields(await current());
    await open(); assert.deepEqual(fields(await current()), before);
    await input("旧备注"); await save(); assert.equal((await current()).note, "旧备注");
  });
  await check("普通状态拖放保留旧备注且同列不写入", async () => {
    await drag("review");
    await until(async () => (await current()).state === "review" && await evaluate("!saving"), "拖放未完成");
    await evaluate("load()");
    assert.equal((await current()).note, "旧备注");
    const before = fields(await current()), posts = await evaluate("optionalStatePosts");
    await drag("review"); await delay(100);
    assert.equal(await evaluate('document.querySelector("#editor").open'), false);
    assert.deepEqual(fields(await current()), before);
    assert.equal(await evaluate("optionalStatePosts"), posts);
  });
  await check("有旧备注时取消阻塞仍保留原状态和备注", async () => {
    const before = fields(await current()), posts = await evaluate("optionalStatePosts");
    await open(); assert.equal(await evaluate('document.querySelector("#note").value'), "旧备注");
    await input(""); await click("#cancel"); await closed();
    assert.deepEqual(fields(await current()), before);
    assert.equal(await evaluate("optionalStatePosts"), posts);
  });
  await check("显式清空旧备注并保存", async () => {
    await open(); await input(""); await save();
    assert.equal((await current()).note, "");
  });
  await check("纯空白由服务统一归一化为空字符串", async () => {
    await open(); await input(" \t\n　"); await save();
    assert.equal((await current()).note, ""); assert.equal((await current()).manual, true);
  });
  await check("失败后清空重试且空草稿可重新打开", async () => {
    await open(); await input("旧草稿");
    await evaluate(`window.fetch=async function(url,options){if(String(url)==="/api/state"){
      optionalStatePosts++;return new Response(JSON.stringify({error:"实验：保存失败"}),{status:409,headers:{"Content-Type":"application/json"}});
    }return optionalFetch.call(this,url,options)};`);
    const before = fields(await current());
    await click("#save");
    await until(() => evaluate('!saving && document.querySelector("#notice").textContent.includes("保存失败")'), "未收到预期错误");
    await input(""); await click("#save");
    await until(() => evaluate('!saving && document.querySelector("#notice").textContent.includes("保存失败")'), "清空后未收到预期错误");
    assert.equal(await evaluate(`noteDraft(${JSON.stringify(report.id)})`), "");
    await escape(); await closed(); await open();
    assert.equal(await evaluate('document.querySelector("#note").value'), "");
    assert.deepEqual(fields(await current()), before);
    await evaluate("window.fetch=window.optionalFetch"); await save();
    assert.equal(await evaluate(`noteDraft(${JSON.stringify(report.id)})`), null);
  });
  await check("保存进行中禁用取消并阻止Escape", async () => {
    await open(); await input("");
    await evaluate(`window.fetch=async function(url,options){if(String(url)==="/api/state")
      await new Promise(resolve=>window.releaseOptionalSave=resolve);return optionalFetch.call(this,url,options)};`);
    await click("#save");
    await until(() => evaluate('typeof releaseOptionalSave==="function" && saving'), "未进入等待保存");
    await escape();
    assert.equal(await evaluate('document.querySelector("#editor").open && document.querySelector("#save").disabled && document.querySelector("#cancel").disabled'), true);
    await evaluate("releaseOptionalSave()"); await closed();
  });
  report.final = fields(await current());
  report.passed = true;
} catch (error) {
  report.error = error.stack;
  if (page) {
    report.page = await evaluate('({dragging:dragging?.id,html:document.querySelector(".board")?.outerHTML,notice:document.querySelector("#notice").textContent})');
  }
  process.exitCode = 1;
} finally {
  report.cleanup = await browser.close();
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output, JSON.stringify(report, null, 2) + "\n");
}
console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, output, error: report.error, cleanup: report.cleanup }));
