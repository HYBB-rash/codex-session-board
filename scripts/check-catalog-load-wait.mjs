import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";
import { fixtureLoadExpression } from "./browser-fixture.mjs";

const deferred = () => {
  let resolve;
  const promise = new Promise(complete => { resolve = complete; });
  return { promise, resolve };
};
const html = (await fs.readFile(new URL("../index.html", import.meta.url), "utf8"))
  .replace("<script>", `<script>
    const originalInterval = window.setInterval;
    window.setInterval = (callback, milliseconds, ...argumentsList) => {
      if (milliseconds !== 4000) return originalInterval(callback, milliseconds, ...argumentsList);
      window.fixturePoll = () => callback(...argumentsList);
      return 0;
    };
  </script><script>`);
assert(html.includes("window.fixturePoll"));
const fixture = {
  fixtureRevision: 0, states: { unknown: "待判断", review: "待审阅", to_archive: "待归档" },
  health: {
    remoteRegistry: [{ hostId: "a", hostLabel: "a", enabled: true }],
    remoteConnections: [{ hostId: "a", connected: true, runtimeVerified: true,
      catalog: { cached: false, complete: true, capturedAt: 1700000000 } }],
  },
  projectCatalog: { loaded: true, items: [{
    id: "server", hostId: "a", remote: true, origin: "targetServer",
    label: "隔离项目", paths: ["/target/server"], canCreate: true,
  }] },
  sessions: [],
};
const writes = [], gates = [], events = [];
const firstRequested = deferred(), secondRequested = deferred(), writeReceived = deferred();
const firstSettled = deferred();
let controlled = false, browser, page, failure, cleanup;
const server = http.createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.url === "/fixture/settled") {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end("{}");
    firstSettled.resolve(JSON.parse(body).serial);
    return;
  }
  if (request.method === "POST") {
    writes.push({ path: request.url, body: JSON.parse(body) });
    writeReceived.resolve();
  }
  const snapshot = JSON.stringify(request.method === "POST" ? { ok: true } : fixture);
  if (controlled && request.url === "/api/sessions") {
    const gate = deferred();
    gate.serial = Number(request.headers["x-fixture-serial"]);
    gates.push(gate);
    events.push({ event: "response-held", request: gates.length, serial: gate.serial, snapshot: JSON.parse(snapshot) });
    if (gates.length === 1) firstRequested.resolve();
    if (gates.length === 2) secondRequested.resolve();
    await gate.promise;
    events.push({ event: "response-released", request: gates.indexOf(gate) + 1 });
  }
  response.writeHead(200, { "Content-Type": request.url === "/" ? "text/html" : "application/json",
    "Cache-Control": "no-store" });
  response.end(request.url === "/" ? html : snapshot);
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
try {
  browser = await launchBrowser();
  page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(`(async () => {
    if (document.readyState !== "complete")
      await new Promise(resolve => addEventListener("load", resolve, { once: true }));
    await load();
  })()`);
  assert.equal(await page.evaluate("canCreateProject(data.projectCatalog.items[0])"), true);
  await page.evaluate(`(() => {
    const originalFetch = fetch;
    window.fetch = (url, options = {}) => originalFetch(url, {
      ...options, headers: { ...options.headers, "X-Fixture-Serial": String(requestSerial) }
    });
  })()`);
  fixture.fixtureRevision = 1;
  fixture.health.remoteConnections[0].connected = false;
  controlled = true;
  await page.evaluate("window.fixtureAttempted = false");
  const expression = fixtureLoadExpression(fixture.fixtureRevision, "pending");
  await page.command("Runtime.evaluate", { expression: `window.fixtureAttempt = (async () => {
    const pending = load(), serial = requestSerial;
    pending.then(() => fetch("/fixture/settled", {
      method: "POST", body: JSON.stringify({ serial })
    }));
    await ${expression};
    window.fixtureAttempted = true;
    await createSession("server");
  })(); void fixtureAttempt.catch(() => {})`, returnByValue: true, awaitPromise: false });
  await firstRequested.promise;
  events.push({ event: "explicit-request-received" });
  await page.evaluate("void fixturePoll()");
  events.push({ event: "poll-started" });
  await secondRequested.promise;
  events.push({ event: "poll-request-received" });
  gates[0].resolve();
  const settledSerial = await firstSettled.promise;
  assert.equal(gates[0].serial, settledSerial);
  assert(gates[1].serial > settledSerial);
  const checkpoint = await page.evaluate(`({
    attempted: fixtureAttempted, revision: data.fixtureRevision, serial: requestSerial,
    connected: data.health.remoteConnections[0].connected
  })`);
  events.push({ event: "superseded-load-settled", settledSerial, ...checkpoint });
  if (checkpoint.attempted) await writeReceived.promise;
  assert.deepEqual(writes, []);
  assert.equal(checkpoint.attempted, false);
  assert.equal(checkpoint.revision, 0);
  assert.equal(checkpoint.connected, true);
  assert(checkpoint.serial > settledSerial);
  controlled = false;
  for (const gate of gates) gate.resolve();
  await page.evaluate("fixtureAttempt");
  assert.deepEqual(writes, []);
  assert.deepEqual(await page.evaluate("[data.fixtureRevision,data.health.remoteConnections[0].connected]"), [1, false]);
  events.push({ event: "latest-snapshot-applied", writes: writes.length });
} catch (error) {
  failure = error;
  if (page) events.push({ event: "failure-state", state: await page.evaluate(
    "({revision:data.fixtureRevision,serial:requestSerial,error:snapshotError,attempted:fixtureAttempted,locked:filtersLocked(),visibility:document.visibilityState})"
  ) });
} finally {
  controlled = false;
  for (const gate of gates) gate.resolve();
  cleanup = await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({ passed: !failure, error: failure?.message, events, writes, cleanup }));
if (failure) throw failure;
