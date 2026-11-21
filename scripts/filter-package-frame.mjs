import assert from "node:assert/strict";
import http from "node:http";
import { archiveFixture } from "./archive-browser-fixture.mjs";
import { until } from "./browser-actions.mjs";
import { checkFilterArchiveScope, filterSnapshot } from "./filter-archive-scope.mjs";

const filterHost = `<!doctype html><meta charset="utf-8">
<iframe sandbox="allow-scripts" src="/filter-resource"></iframe>
<script>
const routes = {board_snapshot:["GET","/api/sessions"], board_archive_batches:["GET","/api/archive-batches"],
  board_archive_batch_start:["POST","/api/archive-batches"], board_archive:["POST","/api/archive"],
  board_archive_batch_finish:["POST","/api/archive-batches/finish"], board_plugin_update_status:["GET","/api/plugin-update"]};
addEventListener("message",async event=>{
  if (event.source !== document.querySelector("iframe").contentWindow) return;
  const {method,id,params} = event.data;
  let result;
  if (method === "ui/initialize") result = {protocolVersion:params.protocolVersion,
    hostInfo:{name:"filter-check",version:"1"},hostContext:{},hostCapabilities:{}};
  if (method === "tools/call") {
    const [verb,path] = routes[params.name];
    const response = await fetch(path,{method:verb,body:verb === "POST" ? JSON.stringify(params.arguments) : undefined});
    result = {content:[],structuredContent:await response.json()};
  }
  if (result) event.source.postMessage({jsonrpc:"2.0",id,result},"*");
});
</script>`;

async function filterFrame(browser, address) {
  await browser.page(address + "/filter-host");
  let child;
  await until(async () => {
    child = (await browser.command("Target.getTargets")).targetInfos
      .find(target => target.type === "iframe" && target.url === address + "/filter-resource");
    return child;
  }, "未找到筛选首件的包内资源 iframe");
  const {sessionId} = await browser.command("Target.attachToTarget",{targetId:child.targetId,flatten:true});
  return async expression => {
    const value = await browser.command("Runtime.evaluate",{expression,awaitPromise:true,returnByValue:true},sessionId);
    assert(!value.exceptionDetails,JSON.stringify(value.exceptionDetails));
    return value.result.value;
  };
}

export async function checkPackagedFilterFrame(browser, resource) {
  const archive = archiveFixture(), writes = [];
  const server = http.createServer(async (request,response) => {
    if (["/filter-host","/filter-resource"].includes(request.url)) {
      return response.writeHead(200,{"Content-Type":"text/html; charset=utf-8"}).end(
        request.url === "/filter-host" ? filterHost : resource);
    }
    if (request.method === "GET" && request.url === "/api/sessions") {
      return response.writeHead(200,{"Content-Type":"application/json"}).end(JSON.stringify(filterSnapshot));
    }
    let body = "";
    for await (const chunk of request) body += chunk;
    const input = body ? JSON.parse(body) : null;
    if (request.method === "POST") writes.push({path:request.url,body:input});
    if (archive.handle(request.url,request.method,input,response)) return;
    if (request.url === "/api/plugin-update") return response.end('{"available":false}');
    response.writeHead(404).end();
  });
  await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
  try {
    const address = `http://127.0.0.1:${server.address().port}`;
    const batches = await fetch(address + "/api/archive-batches");
    assert.deepEqual(await batches.json(),{batches:[]});
    const frame = await filterFrame(browser,address);
    await until(() => frame('!!document.querySelector("#host-filter option[value=\'host:host-b\']")'),
      "嵌入资源没有读到同名主机");
    return await checkFilterArchiveScope(frame,writes);
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
}
