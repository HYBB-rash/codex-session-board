import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";
import { beforeBoardScript } from "./browser-fixture.mjs";
import { until } from "./browser-actions.mjs";

const fixture = new URL(process.argv[2]);
assert.equal(fixture.hostname, "127.0.0.1");
const snapshot = await (await fetch(new URL("/api/sessions", fixture))).json();
const controlPoll = `
  const original = window.setInterval;
  window.setInterval = (callback, delay, ...params) => {
    if (delay !== 2000) return original(callback, delay, ...params);
    window.updatePoll = () => callback(...params);
    return 0;
  };
`;
const html = beforeBoardScript(await (await fetch(fixture)).text(), controlPoll);
const resource = beforeBoardScript(await readFile(process.argv[3], "utf8"), controlPoll);
const host = `<!doctype html><iframe sandbox="allow-scripts" src="/resource"></iframe><script>
addEventListener("message", async event => {
  if (event.source !== document.querySelector("iframe").contentWindow) return;
  const {method,id,params}=event.data;
  let result;
  if (method === "ui/initialize") result = {protocolVersion:params.protocolVersion,
    hostInfo:{name:"isolated update fixture",version:"1"},hostCapabilities:{},hostContext:{}};
  if (method === "tools/call") result = await (await fetch("/tools",{method:"POST",body:JSON.stringify(params)})).json();
  if (result) event.source.postMessage({jsonrpc:"2.0", id, result}, "*");
});
</script>`;
const runningVersion = "0.4.20", version = "0.4.21";
const status = (phase, extra = {}) => ({ available:true, runningVersion, version, phase, ...extra });
const idle = status("idle");
let calls = [], reads = [], writes = [];
const gates = [];
const deferred = () => {
  let release;
  const promise = new Promise(resolve => { release = resolve; });
  gates.push(release);
  return { promise, release };
};
const update = (method, body) => {
  calls.push({ method, body });
  return (method === "GET" ? reads : writes).shift() ?? idle;
};
const server = http.createServer(async (incoming, response) => {
  const document = {"/":html, "/host":host, "/resource":resource}[incoming.url];
  if (document) return response.writeHead(200, {"Content-Type":"text/html; charset=utf-8"}).end(document);
  const chunks = [];
  for await (const chunk of incoming) chunks.push(chunk);
  const body = chunks.length ? JSON.parse(Buffer.concat(chunks)) : null;
  let result;
  if (incoming.url === "/api/plugin-update") {
    result = await update(incoming.method, body);
  } else if (incoming.url === "/tools") {
    const routes = {board_snapshot:snapshot, board_archive_batches:{batches:[]}};
    const method = {board_plugin_update_status:"GET",board_plugin_update_start:"POST"}[body.name];
    const value = method ? await update(method, method === "POST" ? body.arguments : null) : routes[body.name];
    result = value?.errorResponse
      ? {isError:true,content:[{type:"text",text:value.errorResponse}]}
      : {content:[],structuredContent:value};
  } else {
    result = {"/api/sessions":snapshot, "/api/archive-batches":{batches:[]}}[incoming.url];
  }
  const code = result?.errorResponse ? 503 : result ? 200 : 404;
  response.writeHead(code, {"Content-Type":"application/json"}).end(JSON.stringify(
    result?.errorResponse ? {error:result.errorResponse} : result ?? {}));
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser;
const surfaces = [];
try {
  browser = await launchBrowser();
  const origin = `http://127.0.0.1:${server.address().port}`;
  for (const surface of ["http", "mcp"]) {
    calls = []; reads = []; writes = [];
    const page = await browser.page(origin + (surface === "http" ? "/" : "/host"));
    let evaluate = expression => page.evaluate(expression);
    if (surface === "mcp") {
      let child;
      await until(async () => child = (await browser.command("Target.getTargets")).targetInfos
        .find(target => target.type === "iframe" && target.url === origin + "/resource"), "嵌入更新页未加载");
      const {sessionId} = await browser.command("Target.attachToTarget",{targetId:child.targetId,flatten:true});
      evaluate = async expression => {
        const value = await browser.command("Runtime.evaluate", {expression, returnByValue:true, awaitPromise:true}, sessionId);
        assert(!value.exceptionDetails, JSON.stringify(value.exceptionDetails));
        return value.result.value;
      };
    }
    const button = `document.querySelector("#plugin-update")`;
    const text = `document.querySelector("#plugin-update-status").textContent`;
    const postCount = () => calls.filter(call => call.method === "POST").length;
    const click = () => evaluate(`${button}.click()`);
    const waitFor = message => until(() => evaluate(`${text}.includes(${JSON.stringify(message)})`), `${surface} 未显示 ${message}`);
    await until(() => evaluate(`!${button}?.hidden && typeof updatePoll === "function"`), `${surface} 更新按钮未开启`);
    const stale = deferred();
    reads.push(stale.promise);
    await evaluate("window.staleUpdate = updatePoll();void 0");
    await until(() => calls.length === 2, `${surface} 未开始状态轮询`);
    const pending = deferred();
    writes.push(pending.promise);
    await click();
    await until(() => postCount() === 1, `${surface} 未提交更新`);
    assert(await evaluate(`${button}.disabled`));
    await click();
    pending.release(status("fetching",{busy:true}));
    await waitFor("正在获取候选包");
    stale.release(status("failed",{error:"迟到回执"}));
    await evaluate("staleUpdate");
    assert(await evaluate(`${button}.disabled && ${text}.includes("正在获取候选包")`));
    reads.push(status("awaiting_desktop_reload",{error:"隔离重载未完成"}));
    await evaluate("updatePoll()");
    await waitFor("已安装，待重载");
    assert(await evaluate(`!${button}.disabled && ${text}.includes("${runningVersion}") && ${text}.includes("隔离重载未完成")`));
    writes.push({errorResponse:"隔离更新请求失败"});
    await click();
    await waitFor("隔离更新请求失败");
    assert(await evaluate(`!${button}.disabled`));
    writes.push(status("failed",{failedStage:"installing",error:"隔离校验失败"}));
    await click();
    await waitFor("未能确认新包安装完成");
    assert(await evaluate(`${text}.includes("隔离校验失败")`));
    writes.push(status("active"));
    await click();
    await waitFor("已生效，看板已恢复");
    assert.deepEqual(calls.filter(call => call.method === "POST").map(call => call.body), [{},{},{},{}]);
    surfaces.push(surface);
    await page.command("Page.navigate", {url:"about:blank"});
  }
} finally {
  for (const release of gates) release(idle);
  const cleanup = await browser?.close();
  await new Promise(resolve => server.close(resolve));
  console.log(JSON.stringify({surfaces, cleanup}));
}
