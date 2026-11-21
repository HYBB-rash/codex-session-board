import assert from "node:assert/strict";
import { clickNode, until } from "./browser-actions.mjs";
import { launchBrowser } from "./browser-client.mjs";
import { tenCardArchiveFixture } from "./ten-card-archive-fixture.mjs";

const bulk = "#column-to_archive [data-archive-all]";
const single = id => `.card[data-id="${id}"] [data-action="archive"]`;

export async function closeTab(browser, page) {
  const {targetInfo} = await page.command("Target.getTargetInfo");
  assert.equal((await browser.command("Target.closeTarget",{targetId:targetInfo.targetId})).success,true);
  await until(async () => !(await browser.command("Target.getTargets")).targetInfos
    .some(target => target.targetId === targetInfo.targetId), "原标签页未关闭");
}

async function controls(page) {
  return page.evaluate(`Object.fromEntries([...document.querySelectorAll('.card [data-action="archive"],${bulk}')]
    .filter(button=>button.getClientRects().length)
    .map(button=>[button.closest(".card")?.dataset.id ?? "bulk",{
      text:button.innerText,disabled:button.matches(":disabled") || !!button.closest("[inert]") || button.getAttribute("aria-disabled")==="true"
    }]))`);
}

export async function checkDelayedRestore(html, control) {
  const fixture = await tenCardArchiveFixture(html,{outcomes:["archived","pending"]});
  const report = {control,requests:fixture.ledger,cleanup:[],passed:false};
  const archivePosts = () => fixture.ledger.filter(entry => entry.method === "POST" && entry.url === "/api/archive");
  let browser;
  async function closeBrowser() {
    if (!browser) return;
    const cleanup = await browser.close();
    browser = undefined;
    report.cleanup.push(cleanup);
    assert(cleanup.groupEmpty && cleanup.profileRemoved);
  }
  try {
    browser = await launchBrowser();
    let page = await browser.page(fixture.url);
    await until(() => page.evaluate(`document.querySelectorAll(".card").length===10 &&
      document.querySelector('${bulk}')?.disabled===false`),"十卡未就绪");
    await clickNode(page,bulk);
    await until(() => archivePosts().length === 2,"第二项未提交到夹具");
    const batch = fixture.batch();
    assert.deepEqual(batch.items.map(item => item.status),["archived","pending",...Array(8).fill("awaiting")]);
    report.batchId = batch.id;
    report.pendingId = batch.items[1].id;
    await closeTab(browser,page);
    fixture.release();
    await closeBrowser();

    fixture.holdHistory();
    const reopenedAt = fixture.ledger.length;
    browser = await launchBrowser();
    page = await browser.page(fixture.url);
    await until(() => fixture.ledger.slice(reopenedAt).some(entry => entry.method === "GET" && entry.url === "/api/archive-batches"),
      "重开后未读批次");
    await until(() => page.evaluate(`performance.getEntriesByType("resource").some(resource=>
      new URL(resource.name).pathname==="/api/sessions" && resource.responseEnd>0)`),"会话列表未先返回新页面");
    await page.evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
    report.loadingControls = await controls(page);
    const target = control === "单卡" ? report.pendingId : "bulk";
    if (report.loadingControls[target]) await clickNode(page,control === "单卡" ? single(target) : bulk);
    await page.evaluate(`fetch("/test-click-observed").then(response=>response.text())`);
    const writesSinceReopen = () => fixture.ledger.slice(reopenedAt).filter(entry =>
      entry.method === "POST" && entry.url !== "/api/archive-batches/check");
    if (writesSinceReopen().some(entry => entry.url === "/api/archive-batches" &&
      entry.body.items.some(item => item.id === report.pendingId))) {
      await until(() => writesSinceReopen().some(entry => entry.url === "/api/archive" && entry.body.id === report.pendingId),
        "读回前已把待核对项编入新批次");
    }
    report.beforeRead = writesSinceReopen();
    assert.equal(report.beforeRead.length,0,`${control}在初次读回前发出了写请求`);
    assert(Object.values(report.loadingControls).every(button => button.disabled),`${control}场景在初次读回前仍有可用归档按钮`);

    fixture.releaseHistory();
    const writable = batch.items.slice(2).map(item => item.id);
    await until(async () => {
      report.readyControls = await controls(page);
      return writable.every(id => report.readyControls[id]?.disabled === false) && report.readyControls.bulk?.disabled === false
        && (!report.readyControls[report.pendingId] || report.readyControls[report.pendingId].disabled);
    },"明确读回后未恢复八张可写卡，或待核对卡仍可归档");
    const expected = control === "单卡" ? writable.slice(0,1) : writable;
    fixture.holdNextBatchFor(expected);
    await clickNode(page,control === "单卡" ? single(expected[0]) : bulk);
    await until(() => writesSinceReopen().some(entry => entry.url === "/api/archive-batches"),"读回后真实点击未另行建批");
    assert.deepEqual(fixture.errors,[]);
    report.nextBatch = writesSinceReopen().find(entry => entry.url === "/api/archive-batches").body;
    assert.equal(archivePosts().length,2);
    report.passed = true;
  } catch (error) {
    report.failure = error.message;
  } finally {
    try { await closeBrowser(); } finally { await fixture.close(); }
  }
  return report;
}
