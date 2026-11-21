import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import http from "node:http";
import { clickNode, until } from "./browser-actions.mjs";
import { launchBrowser } from "./browser-client.mjs";
import { loadFixture } from "./browser-fixture.mjs";

const html = await readFile(process.argv[2] || new URL("../index.html", import.meta.url));
const row = (id, updatedAt, state = "review") => ({ id, title: id, updatedAt, state, version: 0, inputVersion: 0 });
const snapshot = {
  states: { review: "待审阅", to_archive: "待归档" }, fourColumns: true,
  health: {}, projectCatalog: { loaded: true, items: [] },
  columnSorts: { review: { field: "updated_at", direction: "desc", version: 3 } },
  sessions: [row("z", 9), row("b", 2), row("a", 2), row("invalid", 0),
    row("unplaced-old", 1, null), row("unplaced-new", 2, null)],
};
const writes = [];
const server = http.createServer(async (request, response) => {
  if (request.url === "/") return response.writeHead(200, { "Content-Type": "text/html;charset=utf-8" }).end(html);
  if (request.url === "/api/column-sort") {
    let body = "";
    for await (const chunk of request) body += chunk;
    const change = JSON.parse(body);
    writes.push(change);
    const prior = snapshot.columnSorts[change.state];
    if (prior.version !== change.version) return response.writeHead(409).end(JSON.stringify({ error: "排序版本冲突" }));
    const preference = { field: change.field, direction: change.direction, version: prior.version + 1 };
    snapshot.columnSorts[change.state] = preference;
    return response.end(JSON.stringify({ preference }));
  }
  response.end(JSON.stringify(request.url === "/api/sessions" ? snapshot : {}));
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser;
try {
  browser = await launchBrowser();
  const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
  const evaluate = page.evaluate;
  const wait = expr => until(() => evaluate(expr), expr);
  async function press(key, number) {
    for (const type of ["keyDown", "keyUp"]) {
      const text = type === "keyDown" ? { Enter: "\r", " ": " " }[key] : undefined;
      await page.command("Input.dispatchKeyEvent", { type, key, code: key === " " ? "Space" : key,
        windowsVirtualKeyCode: number, text, unmodifiedText: text });
    }
  }
  const cards = parent => evaluate(`[...document.querySelectorAll("${parent} .card")].map(node=>node.dataset.id)`);
  await wait('document.querySelector("#column-review .column-sort")');
  await evaluate(`window.trigger=document.querySelector("#column-review .column-sort");
    window.menu=document.querySelector("#sort-review");trigger.focus()`);
  assert.deepEqual(await cards("#column-review"), ["z", "a", "b", "invalid"]);
  assert.deepEqual(await cards("#unplaced-cards"), ["unplaced-new", "unplaced-old"]);
  assert.equal(await evaluate('document.querySelector("#column-to_archive .column-sort").value'), "asc");
  await press("ArrowUp", 38);
  await wait('menu.matches(":popover-open")');
  assert.equal(await evaluate("document.activeElement.dataset.direction"), "desc");
  for (const [key, code, expected] of [["ArrowUp", 38, "asc"], ["ArrowDown", 40, "desc"], ["End", 35, "asc"], ["Home", 36, "desc"]]) {
    await press(key, code);
    assert.equal(await evaluate("document.activeElement.dataset.direction"), expected);
  }
  await press("Escape", 27);
  assert.equal(await evaluate('!menu.matches(":popover-open") && document.activeElement === trigger'), true);
  await press("Enter", 13);
  await press("Tab", 9);
  assert.equal(await evaluate('!menu.matches(":popover-open") && document.activeElement !== trigger'), true);
  await clickNode(page, "#column-review .column-sort");
  await wait('menu.matches(":popover-open") && document.activeElement.dataset.direction === "desc"');
  await press("Enter", 13);
  await wait('!menu.matches(":popover-open") && document.activeElement === trigger');
  assert.equal(writes.length, 0);
  await press("ArrowDown", 40);
  await page.command("Emulation.setDeviceMetricsOverride", { width: 390, height: 320, deviceScaleFactor: 1, mobile: false });
  await wait("innerWidth === 390");
  await evaluate('document.getElementById("board-viewport").scrollLeft=180');
  await wait(`(() => { const anchor=trigger.getBoundingClientRect(), popup=menu.getBoundingClientRect();
    return Math.abs(popup.left-Math.max(8,Math.min(anchor.right-174,innerWidth-182)))<1
      && Math.abs(popup.top-Math.max(8,Math.min(anchor.bottom+6,innerHeight-148)))<1; })()`);
  delete snapshot.states.to_archive;
  await loadFixture(page, snapshot);
  assert.equal(await evaluate('menu.matches(":popover-open") && document.activeElement.dataset.direction === "desc"'), true);
  await press("Escape", 27);
  await press("ArrowDown", 40);
  await wait('menu.matches(":popover-open") && trigger.getAttribute("aria-expanded") === "true"');
  snapshot.states = { to_archive: "待归档", review: "待审阅" };
  await loadFixture(page, snapshot);
  snapshot.states = { review: "待审阅", to_archive: "待归档" };
  await loadFixture(page, snapshot);
  assert.equal(await evaluate('document.querySelector(".column")?.dataset.state'), "review");
  assert.equal(await evaluate('menu.matches(":popover-open") && document.activeElement.dataset.direction === "desc"'), true);
  snapshot.columnSorts.review.version = 4;
  await loadFixture(page, snapshot);
  assert.equal(await evaluate('document.activeElement.dataset.direction === "desc" && menu.matches(":popover-open")'), true);
  await press("End", 35);
  await press(" ", 32);
  await wait('document.getElementById("feedback").textContent.includes("排序版本冲突")');
  assert.deepEqual(writes, [{ state: "review", field: "updated_at", direction: "asc", version: 3 }]);
  assert.deepEqual(await cards("#column-review"), ["z", "a", "b", "invalid"]);
  await wait("document.activeElement === trigger && !trigger.disabled");
  await press("ArrowDown", 40);
  await press("End", 35);
  await press("Enter", 13);
  await wait('trigger.value === "asc" && !trigger.disabled && document.activeElement === trigger');
  assert.deepEqual(writes[1], { state: "review", field: "updated_at", direction: "asc", version: 4 });
  assert.deepEqual(await cards("#column-review"), ["a", "b", "z", "invalid"]);
  await press("ArrowUp", 38);
  assert.equal(await evaluate("document.activeElement.dataset.direction"), "asc");
  await clickNode(page, "#column-review h2");
  await wait('!menu.matches(":popover-open") && trigger.getAttribute("aria-expanded") === "false"');
  for (const state of ["review", "to_archive"]) {
    for (let index = 0; index < 30; index++) snapshot.sessions.push(row(`${state}-${index}`, index + 1, state));
  }
  await loadFixture(page, snapshot);
  const scroll = state => `document.querySelector("#column-${state} .cards").scrollTop`;
  await evaluate(`${scroll("review")}=100;${scroll("to_archive")}=100`);
  assert.deepEqual(await evaluate(`[${scroll("review")},${scroll("to_archive")}]`), [100, 100]);
  await clickNode(page, "#column-review .column-sort");
  await press("Home", 36);
  await press("Enter", 13);
  await wait('trigger.value === "desc" && !trigger.disabled');
  assert.deepEqual(await evaluate(`[${scroll("review")},${scroll("to_archive")}]`), [0, 100]);
  snapshot.states = { to_archive: "待归档", review: "待审阅" };
  await loadFixture(page, snapshot);
  snapshot.states = { review: "待审阅", to_archive: "待归档" };
  await loadFixture(page, snapshot);
  assert.equal(await evaluate('document.activeElement === trigger'), true);
} finally {
  try { if (browser) await browser.close(); }
  finally { await new Promise(resolve => server.close(resolve)); }
}
