import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";

const bundle = await fs.readFile(new URL("../assets/app-bridge.js", import.meta.url), "utf8");
const app = `<!doctype html><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none';script-src 'unsafe-inline';style-src 'unsafe-inline'">
<style>:root{color-scheme:light dark}body{background:light-dark(#fff,#1c1d20)}</style>
<script>${bundle.replaceAll("</script", "<\\/script")}</script>`;
const host = `<!doctype html><meta charset="utf-8"><script>
window.requests=[];window.mode="structured";
addEventListener("message",event=>{
  if(event.source!==document.querySelector("iframe")?.contentWindow)return;
  const message=event.data;if(!message?.jsonrpc)return;
  let result;
  if(message.method==="ui/initialize")result={protocolVersion:message.params.protocolVersion,
    hostInfo:{name:"isolated-bridge-check",version:"1"},hostCapabilities:{},
    hostContext:{theme:"dark",safeAreaInsets:{top:12,right:4,bottom:6,left:8}}};
  if(message.method==="tools/call"){
    requests.push(message.params);
    result=message.params.name==="board_preview_reply"?{content:[],_meta:{previewReply:{turnId:"two",html:"<p>本轮答复</p>",inProgress:false}}}:
      mode==="structured"?{content:[],structuredContent:{ok:true,received:message.params.arguments}}:
      mode==="text"?{content:[{type:"text",text:'{"ok":true}'}]}:
      mode==="error"?{isError:true,content:[{type:"text",text:"fixture-denied"}]}:
      mode==="malformed"?{content:[{type:"text",text:"invalid-json"}]}:{content:[]};
  }
  if(result)event.source.postMessage({jsonrpc:"2.0",id:message.id,result},"*");
});
window.notify=params=>document.querySelector("iframe").contentWindow.postMessage({
  jsonrpc:"2.0",method:"ui/notifications/host-context-changed",params},"*");
</script><iframe src="/app" sandbox="allow-scripts"></iframe>`;
const network = [];
const server = http.createServer((request, response) => {
  network.push({ method: request.method, path: request.url });
  if (request.method !== "GET" || !["/host", "/app", "/favicon.ico"].includes(request.url))
    return response.writeHead(403).end();
  response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  response.end(request.url === "/host" ? host : request.url === "/app" ? app : "");
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser, cases = 0, cleanup;
const check = (actual, expected) => { assert.deepEqual(actual, expected); cases++; };
try {
  browser = await launchBrowser();
  const address = `http://127.0.0.1:${server.address().port}`;
  const page = await browser.page(address + "/host");
  let child;
  for (let attempt = 0; attempt < 100; attempt++) {
    child = (await browser.command("Target.getTargets")).targetInfos
      .find(target => target.type === "iframe" && target.url === address + "/app");
    if (child) break;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert(child, "未找到本次隔离 iframe");
  const { sessionId } = await browser.command("Target.attachToTarget", { targetId: child.targetId, flatten: true });
  const evaluate = async expression => {
    const result = await browser.command("Runtime.evaluate",
      { expression, returnByValue: true, awaitPromise: true }, sessionId);
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const wait = async expression => {
    for (let attempt = 0; attempt < 100; attempt++) {
      if (await evaluate(expression)) return;
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    assert.fail("桥接状态未就绪：" + expression);
  };
  await wait('typeof window.boardRequest === "function"');
  check(await evaluate('boardRequest("/api/sessions")'), { ok: true, received: {} });
  check(await evaluate('getComputedStyle(document.documentElement).colorScheme'), "dark");
  check(await evaluate('document.documentElement.style.getPropertyValue("--host-inset-top")'), "12px");
  for (const theme of ["light", "dark"]) {
    await page.evaluate(`notify({theme:${JSON.stringify(theme)}})`);
    await wait(`getComputedStyle(document.documentElement).colorScheme===${JSON.stringify(theme)}`);
    check(await evaluate('getComputedStyle(document.body).backgroundColor'),
      theme === "light" ? "rgb(255, 255, 255)" : "rgb(28, 29, 32)");
  }
  await page.evaluate('notify({safeAreaInsets:{top:0,right:0,bottom:0,left:0}})');
  await wait('document.documentElement.style.getPropertyValue("--host-inset-top")==="0px"');
  check(await evaluate('getComputedStyle(document.documentElement).colorScheme'), "dark");
  const routes = {
    "/api/sessions": "board_snapshot", "/api/state": "board_update", "/api/open": "board_open",
    "/api/refresh": "board_refresh", "/api/new": "board_new", "/api/new/reset": "board_new_reset",
    "/api/connect": "board_connect", "/api/archive": "board_archive", "/api/column-sort": "board_sort",
    "/api/ssh-setup": "board_ssh_setup",
    "/api/project/assign": "board_project_assign",
  };
  for (const [route, name] of Object.entries(routes)) {
    const body = route === "/api/new" ? {
      projectId: 'target:["remote:fixture","targetServer","canonical"]',
      operationId: "019a1111-2222-4333-8444-555555555555",
    } : { id: "fixture", version: 3, hostId: "host-fixture" };
    check(await evaluate(`boardRequest(${JSON.stringify(route)},${JSON.stringify(body)})`), { ok: true, received: body });
    check(await page.evaluate("(({name,arguments:body})=>({name,arguments:body}))(requests.at(-1))"),
      { name, arguments: body });
  }
  const previous = await page.evaluate("requests.length");
  check(await evaluate('boardRequest("toString").then(()=>false, error=>error.message)'), "未知看板操作");
  check(await page.evaluate("requests.length"), previous);
  check(await evaluate('boardRequest("/api/preview-reply",{id:"fixture"})'), {turnId:"two",html:"<p>本轮答复</p>",inProgress:false});
  check(await page.evaluate("(({name,arguments:body})=>({name,arguments:body}))(requests.at(-1))"),
    {name:"board_preview_reply",arguments:{id:"fixture"}});
  for (const [mode, expected] of [["text", { ok: true }], ["error", "fixture-denied"], ["empty", "看板返回了空结果"]]) {
    await page.evaluate(`mode=${JSON.stringify(mode)}`);
    check(await evaluate('boardRequest("/api/sessions").catch(error=>error.message)'), expected);
  }
  await page.evaluate('mode="malformed"');
  check(await evaluate('boardRequest("/api/sessions").then(()=>false,error=>error instanceof SyntaxError)'), true);
  check(network.every(request => request.method === "GET" && !request.path.startsWith("/api/")), true);
} finally {
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({ cases, network, cleanup }));
