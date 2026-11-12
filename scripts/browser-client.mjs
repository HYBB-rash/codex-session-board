import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";

export async function launchBrowser() {
  assert.equal(typeof WebSocket, "function", "需要 Node.js 22 或更新版本");
  const candidates = process.env.CODEX_BOARD_BROWSER ? [process.env.CODEX_BOARD_BROWSER] :
    ["google-chrome", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
  const executable = candidates.find(candidate => spawnSync(candidate, ["--version"], { timeout: 5000 }).status === 0);
  assert(executable, "需要 Chrome/Chromium，可用 CODEX_BOARD_BROWSER 指定");
  const profile = await fs.mkdtemp(path.join(os.tmpdir(), "board-isolation-chrome-"));
  const chrome = spawn(executable, ["--headless=new", "--no-sandbox", "--disable-gpu",
    "--disable-dev-shm-usage", "--no-first-run", "--no-default-browser-check",
    "--disable-background-networking", "--disable-extensions", "--remote-debugging-address=127.0.0.1",
    "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"], { stdio: "ignore", detached: true });
  let startupError, socket, port, serial = 0, stopped;
  const exited = new Promise(resolve => {
    chrome.once("exit", (code, signal) => resolve({ code, signal }));
    chrome.once("error", error => { startupError = error; resolve({ error: error.message }); });
  });
  const pending = new Map(), events = new Set();
  const command = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++serial;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP 超时：${method}`)); }, 10000);
    pending.set(id, { resolve, reject, timer });
    try { socket.send(JSON.stringify({ id, method, params, sessionId })); }
    catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
  });
  const fail = error => {
    for (const waiter of pending.values()) { clearTimeout(waiter.timer); waiter.reject(error); }
    pending.clear();
    for (const waiter of events) { clearTimeout(waiter.timer); waiter.reject(error); }
    events.clear();
  };
  const close = () => stopped ||= (async () => {
    if (socket?.readyState === WebSocket.OPEN) {
      try { await command("Browser.close"); } catch {}
    }
    fail(new Error("本次 Chrome 已关闭"));
    socket?.close();
    const signal = name => {
      if (!chrome.pid || chrome.exitCode !== null || chrome.signalCode !== null) return;
      try { process.kill(-chrome.pid, name); } catch (error) { if (error.code !== "ESRCH") throw error; }
    };
    let exit = await Promise.race([exited, delay(5000, null, { ref: false })]);
    if (!exit) { signal("SIGTERM"); exit = await Promise.race([exited, delay(5000, null, { ref: false })]); }
    if (!exit) { signal("SIGKILL"); exit = await Promise.race([exited, delay(5000, null, { ref: false })]); }
    assert(exit, `无法确认 Chrome ${chrome.pid} 退出，保留 profile：${profile}`);
    if (chrome.pid) {
      let survivors;
      for (let attempt = 0; attempt < 50; attempt++) {
        const processes = spawnSync("/bin/ps", ["-axo", "pid=,pgid=,stat="], { encoding: "utf8", timeout: 5000 });
        assert(processes.status === 0, "无法只读核验 Chrome 进程组，保留 profile");
        survivors = processes.stdout.split("\n").map(line => line.trim().split(/\s+/))
          .filter(([, pgid, state]) => Number(pgid) === chrome.pid && state && !state.startsWith("Z"))
          .map(([pid]) => Number(pid));
        if (!survivors.length) break;
        await delay(100);
      }
      assert(survivors.length === 0, `Chrome 进程组 ${chrome.pid} 仍有存活进程 ${survivors.join(",")}，保留 profile`);
    }
    await fs.rm(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    return { pid: chrome.pid, pgid: chrome.pid, exited: true, ...exit, groupEmpty: true, profileRemoved: true };
  })();
  try {
    let endpoint;
    for (let attempt = 0; attempt < 100 && !endpoint; attempt++) {
      assert(!startupError && chrome.exitCode === null && chrome.signalCode === null, "Chrome 提前退出");
      try {
        const lines = (await fs.readFile(path.join(profile, "DevToolsActivePort"), "utf8")).trim().split("\n");
        if (/^\d+$/.test(lines[0]) && lines[1]?.startsWith("/devtools/browser/")) endpoint = lines;
      } catch (error) { if (error.code !== "ENOENT") throw error; }
      if (!endpoint) await delay(100);
    }
    assert(endpoint, "Chrome 调试端点未启动");
    port = Number(endpoint[0]);
    socket = new WebSocket(`ws://127.0.0.1:${port}${endpoint[1]}`);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("CDP 连接超时")), 10000);
      socket.addEventListener("open", () => { clearTimeout(timer); resolve(); }, { once: true });
      socket.addEventListener("error", () => { clearTimeout(timer); reject(new Error("CDP 连接失败")); }, { once: true });
    });
    socket.addEventListener("close", () => fail(new Error("CDP 连接已断开")));
    socket.addEventListener("message", event => {
      const message = JSON.parse(String(event.data)), waiter = pending.get(message.id);
      if (waiter) {
        pending.delete(message.id); clearTimeout(waiter.timer);
        message.error ? waiter.reject(new Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
      }
      for (const listener of events) if (message.method === listener.method && message.sessionId === listener.sessionId) {
        events.delete(listener); clearTimeout(listener.timer); listener.resolve(message.params);
      }
    });
    return { profile, port, pid: chrome.pid, command, close, async page(url) {
      const { targetId } = await command("Target.createTarget", { url: "about:blank" });
      const { sessionId } = await command("Target.attachToTarget", { targetId, flatten: true });
      const scoped = (method, params) => command(method, params, sessionId);
      await scoped("Page.enable");
      await scoped("Emulation.setDeviceMetricsOverride", { width: 1680, height: 1000, deviceScaleFactor: 1, mobile: false });
      await scoped("Page.navigate", { url });
      return { command: scoped, async evaluate(expression) {
        const result = await scoped("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
        assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
        return result.result.value;
      }, event: method => new Promise((resolve, reject) => {
        const listener = { method, sessionId, resolve, reject };
        listener.timer = setTimeout(() => { events.delete(listener); reject(new Error(`CDP 事件超时：${method}`)); }, 10000);
        events.add(listener);
      }) };
    } };
  } catch (error) {
    await close();
    throw error;
  }
}
