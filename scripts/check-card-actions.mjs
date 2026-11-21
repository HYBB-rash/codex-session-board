import assert from "node:assert/strict";
import {setTimeout as delay} from "node:timers/promises";
import {launchBrowser} from "./browser-client.mjs";

const browser = await launchBrowser();
try {
  const page = await browser.page("about:blank");
  const wait = async expression => {
    for (let attempt = 0; attempt < 160; attempt++) {
      if (await page.evaluate(expression)) return;
      await delay(50);
    }
    assert.fail(expression);
  };
  const checkSubmitting = async (path, click) => {
    await page.command("Fetch.enable", {patterns:[{urlPattern:"*/api/" + path,requestStage:"Request"}]});
    const paused = page.event("Fetch.requestPaused");
    await page.evaluate(click);
    const {requestId,request} = await paused;
    assert.equal(request.method, "POST");
    assert.equal(await page.evaluate(`document.querySelector("[data-archive-all]").disabled &&
      [...document.querySelectorAll(".card")].every(row => !row.draggable &&
        row.querySelector("[data-action=adjust]").disabled && !row.querySelector("[data-action=archive]:enabled"))`), true);
    await page.evaluate('document.querySelector("[data-archive-all]").click()');
    await page.command("Fetch.continueRequest", {requestId});
    await page.command("Fetch.disable");
  };
  await page.command("Fetch.enable", {patterns:[{urlPattern:"*/api/archive-batches",requestStage:"Request"}]});
  const history = page.event("Fetch.requestPaused");
  await page.command("Page.navigate", {url:process.argv[2]});
  const pendingRead = await history;
  assert.equal(pendingRead.request.method, "GET");
  await page.evaluate(`window.choose = (name,value) => {
    const select = document.getElementById(name);
    select.value = value;
    select.dispatchEvent(new Event("change"));
  };
  window.card = title => [...document.querySelectorAll(".card")].find(
    row => row.querySelector(".card-title").textContent === title);
  window.buttons = title => {
    const row = card(title);
    return [!row.querySelector("[data-action=adjust]").disabled, row.draggable,
      row.querySelector("[data-action=archive]")?.textContent ?? null];
  };
  choose("host-filter","all");choose("history-filter","cached")`);
  assert.deepEqual(await page.evaluate('buttons("Cache")'), [false,false,null]);
  await page.evaluate('choose("history-filter","all")');
  assert.equal(await page.evaluate('card("Pending").textContent.includes("归档结果待核对")'), true);
  assert.deepEqual(await page.evaluate('["Local","Online","Archived","Catalog","Follow","Pending"].map(buttons)'), [
    [true,true,"归档"],[true,true,"归档"],[false,false,null],[false,false,null],[true,true,"确认待归档"],[true,true,null],
  ]);
  const missing = await page.evaluate(`(() => {
    const {id,version,inputVersion} = data.sessions.find(row => row.title === "Catalog");
    return post("/api/state",{id,version,inputVersion,state:"review"}).then(() => "",error => error.message);
  })()`);
  assert.match(missing, /当前不可修改/);
  await page.command("Fetch.continueRequest", {requestId:pendingRead.requestId});
  await page.command("Fetch.disable");
  await wait('archivePending.size === 1 && !archiveChecking');
  await page.evaluate('card("Online").querySelector("[data-action=adjust]").click()');
  assert.equal(await page.evaluate('byId("editor").open'), true);
  await page.evaluate('byId("cancel").click()');
  await checkSubmitting("archive", 'document.querySelector("[data-archive-all]").click()');
  await wait('!saving && !archiveChecking && card("Online")?.textContent.includes("归档结果待核对")');
  assert.deepEqual(await page.evaluate('buttons("Online")'), [true,true,null]);
  const retry = await page.evaluate(`(() => {
    const {id,version,inputVersion,sectionVersion} = data.sessions.find(row => row.title === "Online");
    return post("/api/archive",{id,version,inputVersion,sectionVersion}).then(() => "",error => error.message);
  })()`);
  assert.match(retry, /请先在归档结果中核对/);
  await checkSubmitting("state", 'card("Follow").querySelector("[data-action=archive]").click()');
  await wait('!saving && buttons("Follow")[2] === "归档"');
  await page.evaluate('card("Follow").querySelector("[data-action=archive]").click()');
  await wait('!saving && byId("archive-summary-text").textContent.includes("1")');
} finally {
  await browser.close();
}
