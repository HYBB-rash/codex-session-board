import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import http from "node:http";
import { fileURLToPath } from "node:url";

function installFixture() {
  let phase = "request-blocked", releaseRequest, releaseResponse;
  const report = event => process.send({ ...event, phase });
  const createServer = http.createServer;
  http.createServer = function (...argumentsList) {
    const listener = argumentsList.pop();
    assert.equal(typeof listener, "function");
    return createServer.call(this, ...argumentsList, async (request, response) => {
      if (request.method !== "POST" || request.url !== "/api/open") return listener(request, response);
      await new Promise(resolve => {
        releaseRequest = resolve;
        report({ event: "request-held" });
      });
      const end = response.end;
      response.end = function (...parts) {
        phase = "response-blocked";
        releaseResponse = () => end.apply(this, parts);
        response.once("finish", () => {
          phase = "response-complete";
          report({ event: "response-complete" });
        });
        report({ event: "response-held" });
        return this;
      };
      return listener(request, response);
    });
  };
  const deepEqual = assert.deepEqual;
  assert.deepEqual = function (actual, expected, ...rest) {
    if (JSON.stringify(expected) === '[["/api/open","long"]]')
      report({ event: "writes-assertion", actual, expected });
    return deepEqual.call(this, actual, expected, ...rest);
  };
  const BrowserSocket = globalThis.WebSocket;
  globalThis.WebSocket = class extends BrowserSocket {
    constructor(...argumentsList) {
      super(...argumentsList);
      this.waits = new Map();
      this.addEventListener("message", event => {
        const message = JSON.parse(String(event.data)), wait = this.waits.get(message.id);
        if (!wait) return;
        this.waits.delete(message.id);
        report({ event: "wait-observed", ...wait, value: message.result?.result?.value });
      });
    }
    send(text) {
      const message = JSON.parse(text);
      if (message.method === "Runtime.evaluate" && ["!saving", "!opening"].includes(message.params.expression))
        this.waits.set(message.id, { expression: message.params.expression, dispatchedPhase: phase });
      return super.send(text);
    }
  };
  process.on("message", message => {
    if (message.event === "release-request") {
      assert.equal(phase, "request-blocked");
      assert.equal(typeof releaseRequest, "function");
      phase = "request-released";
      releaseRequest();
      releaseRequest = null;
    } else if (message.event === "release-response") {
      assert.equal(phase, "response-blocked");
      assert.equal(typeof releaseResponse, "function");
      phase = "response-released";
      releaseResponse();
      releaseResponse = null;
    } else {
      assert.fail("未知夹具控制消息：" + JSON.stringify(message));
    }
  });
  process.channel.unref();
}

async function runRegression() {
  const started = performance.now(), events = [];
  let stdout = "", stderr = "", requestHeld = false, responseHeld = false;
  let requestBlockedWait = false, responseBlockedWait = false, requestReleased = false, responseReleased = false;
  const child = spawn(process.execPath, ["--import", import.meta.url, "scripts/check-light-ui.mjs"], {
    cwd: fileURLToPath(new URL("../", import.meta.url)),
    env: { ...process.env, CSB_OPEN_WAIT_FIXTURE_CHILD: "1" },
    stdio: ["ignore", "pipe", "pipe", "ipc"],
  });
  child.stdout.on("data", chunk => { stdout += chunk; });
  child.stderr.on("data", chunk => { stderr += chunk; });
  child.on("message", message => {
    events.push(message);
    if (message.event === "request-held") requestHeld = true;
    if (message.event === "response-held") responseHeld = true;
    if (message.event === "wait-observed" && message.value === false && message.dispatchedPhase === message.phase) {
      if (message.phase === "request-blocked") requestBlockedWait = true;
      if (message.phase === "response-blocked") responseBlockedWait = true;
    }
    if (requestHeld && requestBlockedWait && !requestReleased) {
      requestReleased = true;
      events.push({ event: "release-request" });
      child.send({ event: "release-request" });
    }
    if (responseHeld && responseBlockedWait && !responseReleased) {
      responseReleased = true;
      events.push({ event: "release-response" });
      child.send({ event: "release-response" });
    }
  });
  const exit = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
  const report = {
    passed: false, elapsed_seconds: (performance.now() - started) / 1000,
    events, requestBlockedWait, responseBlockedWait,
    child: { ...exit, stdout, stderr },
  };
  try {
    assert.equal(exit.code, 0, stderr);
    assert.equal(exit.signal, null);
    assert.equal(requestBlockedWait, true, "请求处理放行前，完成条件必须保持 false");
    assert.equal(responseBlockedWait, true, "响应放行前，完成条件必须保持 false");
    const assertions = events.filter(event => event.event === "writes-assertion");
    assert.equal(assertions.length, 1);
    assert.equal(assertions[0].phase, "response-complete", "完整写入断言不能早于响应完成");
    assert.equal(events.filter(event => event.event === "response-complete").length, 1);
    assert.equal(events.some(event => event.event === "wait-observed" &&
      ["request-blocked", "response-blocked"].includes(event.phase) && event.value !== false), false);
    const browserReport = JSON.parse(stdout);
    assert.equal(browserReport.cleanup.groupEmpty, true);
    assert.equal(browserReport.cleanup.profileRemoved, true);
    report.passed = true;
  } finally {
    console.log(JSON.stringify(report));
  }
}

if (process.env.CSB_OPEN_WAIT_FIXTURE_CHILD === "1") installFixture();
else await runRegression();
