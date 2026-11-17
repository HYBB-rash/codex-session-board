import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { launchBrowser } from "./browser-client.mjs";
import { until, clickNode, dragCard } from "./browser-actions.mjs";

const source = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
assert(process.argv[2], "需要源码外的报告目录");
const output = path.resolve(process.argv[2]), verifyOnly = process.argv.includes("--verify-only");
assert(!output.startsWith(source + path.sep) && output !== source, "报告必须在源码之外");
const root = path.join(os.homedir(), ".csb-dev", createHash("sha256").update(source).digest("hex").slice(0, 12));
assert.equal(await fs.realpath(root), root);
const runtime = JSON.parse(await fs.readFile(path.join(root, "runtime.json"), "utf8"));
assert.equal(runtime.phase, "running", "请先启动本工作树的隔离开发环境");
const response = await fetch(new URL("/api/info", runtime.url), { redirect: "error" });
const info = await response.json();
assert(response.ok && info.pid === runtime.service_pid && info.build === runtime.program_sha256 &&
  info.dataDir === path.join(root, "data"), "服务身份不属于本工作树，拒绝写入");
const report = { passed: false, source, sourceCommit: runtime.source_sha, fingerprint: runtime.fingerprint,
  runtime: { pid: info.pid, build: info.build, dataDir: info.dataDir, configId: info.configId }, cases: [] };
const read = async () => {
  const response = await fetch(new URL("/api/sessions", runtime.url), { redirect: "error" });
  assert(response.ok, `读回失败：HTTP ${response.status}`);
  const snapshot = await response.json();
  const row = snapshot.sessions.find(row => row.id === report.id);
  assert(row, "fixture 会话消失");return row;
};
if (verifyOnly) {
  const previous = JSON.parse(await fs.readFile(path.join(output, "fixture.json"), "utf8"));
  assert(previous.passed && previous.source === source && previous.fingerprint === runtime.fingerprint);
  report.id = previous.id;
  report.final = await read();
  for (const field of ["state", "note", "manual", "sectionVersion"]) assert.deepEqual(report.final[field], previous.final[field], field);
  assert(report.final.version >= previous.final.version);
  report.passed = true;report.cases.push("停止后重启保留真实状态与备注");
  await fs.writeFile(path.join(output, "restart.json"), JSON.stringify(report, null, 2) + "\n", { flag: "wx" });
  console.log(JSON.stringify({ passed: true, cases: report.cases.length, output }));
  process.exit(0);
}
await fs.mkdir(output);
const browser = await launchBrowser();
let page;
const evaluate = expression => page.evaluate(expression);
const wait = expression => until(() => evaluate(expression), expression);
const check = async (name, expression) => {
  assert(await evaluate(expression), name);report.cases.push(name);
};
const screenshot = async name => {
  const image = await page.command("Page.captureScreenshot", { format: "png" });
  await fs.writeFile(path.join(output, name + ".png"), Buffer.from(image.data, "base64"));
};
const recordMotion = async name => {
  const clip = await evaluate('(() => {const rect=byId("move-feedback").getBoundingClientRect();return {x:rect.x,y:rect.y,width:rect.width,height:rect.height,scale:1};})()');
  for (let index = 0; index < 10; index++) {
    const image = await page.command("Page.captureScreenshot", { format: "png", clip });
    await fs.writeFile(path.join(output, `${name}-${index}.png`), Buffer.from(image.data, "base64"));
    await delay(150);
  }
};
const drag = async state => {
  await evaluate("delete window.fixtureRelease");
  await dragCard(page, report.id, state);
  await wait('typeof fixtureRelease === "function" && saving');
};
const release = async () => {
  await evaluate("fixtureRelease()");
  await wait('!saving && !moving && byId("move-feedback").classList.contains("saved")');
};
try {
  page = await browser.page(runtime.url);
  await wait('typeof data!=="undefined" && data.health.runtimeVerified && data.health.nativeVerified && moveArtReady && data.sessions.some(row=>!row.remote && !row.readOnly && typeof row.sectionVersion==="string")');
  report.id = await evaluate('data.sessions.find(row=>!row.remote && !row.readOnly && typeof row.sectionVersion==="string").id');
  assert.match(report.id, /^[0-9a-f-]{36}$/);
  const before = await read(), target = before.state === "review" ? "unknown" : "review";
  await evaluate('window.fixtureWrites=[];window.fixtureFetch=window.fetch;window.fetch=async function(url,options){const state=String(url)==="/api/state";if(state)await new Promise(resolve=>window.fixtureRelease=resolve);const response=await fixtureFetch.call(this,url,options);if(state)fixtureWrites.push({request:JSON.parse(options.body),status:response.status,body:await response.clone().json()});return response}');
  await drag(target);await wait('!byId("move-feedback").hidden');
  report.waitAppearedAfterMs = await evaluate("moving.shownAt-moving.started");
  assert(report.waitAppearedAfterMs >= 1500);
  assert.equal((await read()).state, before.state);
  await check("真实Rust等待态不冒充服务端已保存", 'saving && moveFeedback===moving && byId("move-feedback").classList.contains("waiting")');
  await screenshot("wait");await recordMotion("waiting");
  await delay(600);
  await check("角色实际可见超过两秒后仍等待回执", 'performance.now()-moving.shownAt>=2000 && !byId("move-feedback").hidden && saving');
  await page.command("Emulation.setDeviceMetricsOverride", { width: 320, height: 568, deviceScaleFactor: 1, mobile: false });
  await check("320乘568窄屏角色与收起按钮均在视口内", '(() => {const panel=byId("move-feedback").getBoundingClientRect(),button=byId("move-dismiss").getBoundingClientRect();return document.documentElement.scrollWidth<=innerWidth && panel.left>=0 && panel.top>=0 && panel.right<=innerWidth && panel.bottom<=innerHeight && button.bottom<=innerHeight;})()');
  await screenshot("narrow");
  await page.command("Emulation.setDeviceMetricsOverride", { width: 1680, height: 1000, deviceScaleFactor: 1, mobile: false });
  await release();
  const saved = await read();
  assert.equal(saved.state, target);assert(saved.version > before.version);
  await check("服务端保存成功且动画期间立即恢复后续操作", '!saving && !moving && !byId("move-feedback").hidden && [...document.querySelectorAll(".card")].every(card=>card.draggable)');
  await evaluate('window.fixtureSuccessAt=moveFeedback.visibleSince;window.fixtureHiddenAt=null;window.fixtureObserver=new MutationObserver(()=>{if(byId("move-feedback").hidden && fixtureHiddenAt===null)fixtureHiddenAt=performance.now()});fixtureObserver.observe(byId("move-feedback"),{attributes:true,attributeFilter:["hidden"]})');
  await screenshot("saved");await recordMotion("success");await wait("fixtureHiddenAt!==null");
  report.successVisibleMs = await evaluate("fixtureHiddenAt-fixtureSuccessAt");
  assert(report.successVisibleMs >= 2000 && report.successVisibleMs < 2500);
  report.cases.push("真实时钟成功画面保留两秒");
  await evaluate("fixtureObserver.disconnect()");
  await drag(target === "review" ? "unknown" : "review");await wait('!byId("move-feedback").hidden');await release();
  await drag(target);await wait('!byId("move-feedback").hidden');await delay(650);
  await check("连续真实拖动跨旧截止点不隐藏新等待", 'performance.now()-moving.started>2000 && !byId("move-feedback").hidden && byId("move-feedback").classList.contains("waiting")');
  await clickNode(page, "#move-dismiss");
  await check("手动收起立即生效且保存继续", 'saving && moving.dismissed && byId("move-feedback").hidden');
  await evaluate("fixtureRelease()");await wait("!saving && !moving");
  assert.equal((await read()).state, target);
  await check("收起后保存成功也不重新弹出角色", 'byId("move-feedback").hidden');
  report.writes = await evaluate("fixtureWrites");
  assert.equal(report.writes.length, 3);
  assert(report.writes.every(write => write.status === 200));
  report.final = await read();report.passed = true;
} catch (error) {
  report.error = { name: error.name, message: error.message, stack: error.stack };
  if (page) {
    report.writes = await evaluate("window.fixtureWrites");
    report.feedback = await evaluate('byId("feedback").textContent');
    report.final = await read();
    await screenshot("failure");
  }
  throw error;
} finally {
  report.cleanup = await browser.close();
  await fs.writeFile(path.join(output, "fixture.json"), JSON.stringify(report, null, 2) + "\n");
}
console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, waitAppearedAfterMs: report.waitAppearedAfterMs,
  successVisibleMs: report.successVisibleMs, cleanup: report.cleanup, output }));
