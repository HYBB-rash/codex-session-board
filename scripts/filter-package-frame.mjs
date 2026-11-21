import assert from "node:assert/strict";
import { until } from "./browser-actions.mjs";

export const filterHost = `<!doctype html><meta charset="utf-8">
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

export async function filterFrame(browser, address) {
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
