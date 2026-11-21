import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import { launchBrowser } from "./browser-client.mjs";
import { clickNode, until } from "./browser-actions.mjs";
import { tenCardArchiveFixture } from "./ten-card-archive-fixture.mjs";

const html = await fs.readFile(process.argv[2] || new URL("../index.html", import.meta.url));
const fixture = await tenCardArchiveFixture(html);
const report = {pageSha256:createHash("sha256").update(html).digest("hex"), nativeArchiveWrites:null,
  evidenceScope:"隔离 Chromium 与 HTTP 夹具。原生归档写入次数由 Mac 原生验收单独记录。", cleanup:[]};
const statuses = {archived:"已归档", verified:"核实时已归档", pending:"待核对", newInput:"未归档 · 有新输入",
  rejected:"未归档", awaiting:"尚无提交记录", submitting:"正在提交", queued:"等候提交"};
const archiveButton = "#column-to_archive [data-archive-all]";
const toggle = "#archive-results-toggle";
const checkButton = "#archive-summary-actions button";
const count = (method, url) => fixture.ledger.filter(entry => entry.method === method && entry.url === url).length;
const submitCount = () => count("POST", "/api/archive");
const checkCount = () => count("POST", "/api/archive-batches/check");
let browser, page;

async function waitFor(expression, message) {
  await until(async () => {
    assert.deepEqual(fixture.errors, []);
    return page.evaluate(expression);
  }, message);
}
async function details() {
  return page.evaluate(`({
    summary:document.querySelector("#archive-summary-text").innerText,
    actions:[...document.querySelectorAll("#archive-summary-actions button")].map(button=>({text:button.innerText,disabled:button.disabled})),
    items:[...document.querySelectorAll("#archive-result-list .archive-batch li")].map(row=>({
      title:row.querySelector("div").firstChild.textContent.trim(),
      status:row.querySelector(".archive-item-status").innerText,
      message:row.querySelector("p")?.innerText ?? null,
      actions:[...row.querySelectorAll("button")].map(button=>button.innerText)
    }))
  })`);
}
async function cardActions() {
  return page.evaluate(`Object.fromEntries([...document.querySelectorAll(".card[data-id]")].map(card=>{
    const button=card.querySelector('[data-action="archive"]');
    return [card.dataset.id,button ? {text:button.innerText,disabled:button.disabled} : null];
  }))`);
}
function recordCounts() {
  return {archivePosts:submitCount(),checkPosts:checkCount(),batchReads:count("GET","/api/archive-batches")};
}
function verifyRows(view, recorded, expected) {
  assert.equal(view.items.length, 10);
  assert.deepEqual(Object.fromEntries(view.items.map(row => [row.title,row.status])),
    Object.fromEntries(recorded.items.map((item,index) => [item.title,statuses[expected[index]]])));
  for (const item of recorded.items) {
    const row = view.items.find(row => row.title === item.title);
    assert.deepEqual(row.actions, item.status === "newInput" ? ["打开会话"] : []);
    if (["newInput","rejected"].includes(item.status)) assert.equal(row.message, item.message);
  }
}
async function openDetails() {
  await waitFor(`!!document.querySelector("${toggle}") && !document.querySelector("${toggle}").hidden`,
    "归档结果按钮未出现");
  await clickNode(page, toggle);
  await waitFor(`document.querySelector("${toggle}").getAttribute("aria-expanded")==="true" &&
    document.querySelectorAll("#archive-result-list li").length===10`, "十条结果明细未打开");
}
async function closeBrowser() {
  if (!browser) return;
  const cleanup = await browser.close();
  browser = undefined;
  assert.equal(cleanup.groupEmpty, true);
  assert.equal(cleanup.profileRemoved, true);
  report.cleanup.push(cleanup);
}
async function run() {
  browser = await launchBrowser();
  page = await browser.page(fixture.url);
  await waitFor(`document.querySelectorAll(".card").length===10 &&
    document.querySelector("${archiveButton}")?.disabled===false`, "十张可归档卡片未就绪");
  await clickNode(page, archiveButton);
  await until(() => {
    assert.deepEqual(fixture.errors, []);
    return submitCount() === 7;
  }, "第七张卡的真实请求未到达夹具");
  await openDetails();
  const beforeClose = fixture.batch();
  const live = await details();
  const prefix = ["archived","archived","archived","archived","newInput","rejected"];
  const recovered = [...prefix,"pending","awaiting","awaiting","awaiting"];
  assert.deepEqual(beforeClose.items.map(item => item.status), recovered);
  verifyRows(live, beforeClose, [...prefix,"submitting","queued","queued","queued"]);
  for (const text of ["正在提交第 7 / 10 条","已确认归档 4 / 10 条","1 条正在提交","3 条等候提交","1 条有新输入","1 条未归档"])
    assert(live.summary.includes(text), live.summary);
  report.inFlight = live;
  report.phases = {beforeClose:recordCounts()};
  const {targetInfo} = await page.command("Target.getTargetInfo");
  assert.equal((await browser.command("Target.closeTarget", {targetId:targetInfo.targetId})).success, true);
  await until(async () => !(await browser.command("Target.getTargets")).targetInfos
    .some(target => target.targetId === targetInfo.targetId), "原标签页未关闭");
  fixture.release();
  await closeBrowser();
  const beforeOpen = recordCounts();
  browser = await launchBrowser();
  page = await browser.page(fixture.url);
  await waitFor(`document.querySelector("#archive-summary-text")?.innerText.includes("1 条待核对")`,
    "新页面未从服务端恢复待核对项");
  await until(() => checkCount() > beforeOpen.checkPosts, "重开后未自动核对待定卡片");
  const action = `Array.from(document.querySelectorAll("${checkButton}")).find(button=>button.innerText==="核对结果")`;
  await waitFor(`(${action})?.disabled===false`, "待核对项没有可用的核对按钮");
  assert(count("GET","/api/archive-batches") > beforeOpen.batchReads);
  await openDetails();
  const restored = await details();
  verifyRows(restored, fixture.batch(), recovered);
  for (const text of ["已确认归档 4 / 10 条","1 条待核对","3 条尚无提交记录","1 条有新输入","1 条未归档"])
    assert(restored.summary.includes(text), restored.summary);
  assert(!restored.summary.includes("正在提交") && !restored.summary.includes("等候提交"));
  assert.deepEqual(restored.actions.map(button => [button.text,button.disabled]).sort(),
    [["打开会话",false],["核对结果",false]].sort());
  restored.cards = await cardActions();
  for (const item of beforeClose.items) {
    if (item.status === "archived") assert(!Object.hasOwn(restored.cards,item.id),item.title);
    else if (item.status === "pending") assert.equal(restored.cards[item.id],null,item.title);
    else assert.deepEqual(restored.cards[item.id],
      {text:item.status === "newInput" ? "确认待归档" : "归档",disabled:false},item.title);
  }
  report.reopened = restored;
  report.phases.reopened = recordCounts();
  const newInput = beforeClose.items.find(item => item.status === "newInput");
  const rowIndex = restored.items.findIndex(row => row.title === newInput.title);
  await clickNode(page, `#archive-result-list li:nth-child(${rowIndex + 1}) button`);
  await until(() => count("POST","/api/open") === 1, "明细打开会话未送达");
  assert.equal(fixture.ledger.find(entry => entry.url === "/api/open").body.id,newInput.id);
  const index = await page.evaluate(`[...document.querySelectorAll("${checkButton}")].indexOf(${action})`);
  const selector = `#archive-summary-actions button:nth-of-type(${index + 1})`;
  const manualStart = fixture.ledger.length;
  const checks = checkCount();
  await clickNode(page, selector);
  await waitFor('document.querySelector("#archive-summary-text").innerText.includes("仍有结果待确认")',
    "手动核对未显示仍未确认的结果");
  assert.equal(checkCount(), checks + 1);
  assert.deepEqual(fixture.batch(), beforeClose);
  report.phases.stillPending = recordCounts();
  fixture.allowVerification();
  await clickNode(page, selector);
  await waitFor('document.querySelector("#archive-summary-text").innerText.includes("已确认归档 5 / 10 条")',
    "原生结果可读后未显示五张已确认卡片");
  const verified = await details();
  verifyRows(verified, fixture.batch(), [...prefix,"verified","awaiting","awaiting","awaiting"]);
  assert(!verified.actions.some(button => button.text === "核对结果"));
  assert.equal(checkCount(), checks + 2);
  assert.deepEqual(fixture.ledger.slice(manualStart).filter(entry => entry.method === "POST").map(entry => entry.url),
    ["/api/archive-batches/check","/api/archive-batches/check"]);
  report.phases.verified = recordCounts();
  const submitted = fixture.ledger.filter(call => call.url === "/api/archive").map(call => call.body.id);
  assert.deepEqual(submitted, beforeClose.items.slice(0,7).map(item => item.id));
  assert.equal(count("POST","/api/archive-batches"),1);
  assert.equal(count("POST","/api/archive-batches/finish"),0);
  assert.deepEqual(fixture.errors, []);
  Object.assign(report, {verified, batchId:beforeClose.id, persistedAtClose:beforeClose.items, submitted,
    archiveHttpCounts:Object.fromEntries(beforeClose.items.map(item => [item.id,submitted.filter(id => id === item.id).length])),
    requests:fixture.ledger, passed:true});
}
try { await run(); }
finally { try { await closeBrowser(); } finally { await fixture.close(); } }
console.log(JSON.stringify(report));
