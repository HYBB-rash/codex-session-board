import assert from "node:assert/strict";
import {randomUUID} from "node:crypto";
import http from "node:http";
import {createInterface} from "node:readline";
import {launchBrowser} from "./browser-client.mjs";
import {until} from "./browser-actions.mjs";
import {checkPackagedEditor} from "./check-editor-drafts.mjs";

const [target,follow] = process.argv.slice(2);
const replies = new Map(), input = createInterface({input:process.stdin});
let nextId = 0;
input.on("line",line => {
  const {id,result,error} = JSON.parse(line);
  replies.get(id)?.(result,error);
  replies.delete(id);
});
const rpc = (method,params) => new Promise((resolve,reject) => {
  const id = ++nextId;
  const timeout = setTimeout(() => reject(new Error("MCP fixture 超时 "+method)), 15000);
  replies.set(id,(result,error) => {
    clearTimeout(timeout);
    if (error) reject(new Error(JSON.stringify(error)));
    else resolve(result);
  });
  process.stdout.write(JSON.stringify({jsonrpc:"2.0",id,method,params})+"\n");
});
await rpc("initialize",{protocolVersion:"2025-06-18",capabilities:{},clientInfo:{name:"isolated-panel",version:"1"}});
process.stdout.write(JSON.stringify({jsonrpc:"2.0",method:"notifications/initialized"})+"\n");
const {resources} = await rpc("resources/list",{});
const {contents:[panel]} = await rpc("resources/read",{uri:resources[0].uri});
assert.deepEqual(panel._meta.ui.csp.connectDomains, []);
const relay = "/"+randomUUID();
const host = `<!doctype html><style>html,body,iframe{margin:0;border:0;width:100%;height:100%}</style><script>
addEventListener("message",async event=>{
  if(event.source!==document.querySelector("iframe")?.contentWindow)return;
  const {method,params,id}=event.data;
  let result;
  if(method==="ui/initialize")result={protocolVersion:params.protocolVersion,hostInfo:{name:"fixture",version:"1"},hostCapabilities:{},hostContext:{}};
  if(method==="tools/call")result=await (await fetch("${relay}",{method:"POST",body:JSON.stringify(params)})).json();
  if(result)event.source.postMessage({jsonrpc:"2.0",id,result},"*");
});
</script><iframe src="/panel" sandbox="allow-scripts"></iframe>`;
const calls = [];
let releaseWrite;
const heldWrite = new Promise(resolve => {releaseWrite = resolve;});
let releaseRefresh;
const heldRefresh = new Promise(resolve => {releaseRefresh = resolve;});
const server = http.createServer(async (request,response) => {
  if (request.method === "POST" && request.url === relay) {
    let text = "";
    for await (const chunk of request) text += chunk;
    const params = JSON.parse(text);
    const result = await rpc("tools/call",params).catch(error => ({
      isError:true,content:[{type:"text",text:error.message}],
    }));
    calls.push({...params,result});
    if (params.name === "board_archive") await heldWrite;
    if (params.name === "board_snapshot" && calls.some(call => call.name === "board_update")) await heldRefresh;
    return response.writeHead(200,{"Content-Type":"application/json"}).end(JSON.stringify(result));
  }
  if (request.url === "/panel") response.setHeader("Content-Security-Policy",
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'");
  response.writeHead(200,{"Content-Type":"text/html; charset=utf-8"}).end(
    request.url === "/" ? host : request.url === "/panel" ? panel.text : "");
});
await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
let browser;
try {
  browser = await launchBrowser();
  const url = `http://127.0.0.1:${server.address().port}`;
  const outer = await browser.page(url);
  let frame;
  await until(async () => Boolean(frame = (await browser.command("Target.getTargets")).targetInfos
    .find(item => item.type === "iframe" && item.url === url+"/panel")), "离线面板未载入");
  const {sessionId} = await browser.command("Target.attachToTarget",{targetId:frame.targetId,flatten:true});
  const evaluate = async expression => {
    const result = await browser.command("Runtime.evaluate",{expression,returnByValue:true},sessionId);
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  try {
    await until(() => evaluate('document.querySelector(".card") !== null'), "未读到卡片");
  } catch (error) {
    console.error(await evaluate('({ready:document.readyState,bridge:typeof boardRequest,load:typeof load,feedback:document.querySelector("#feedback")?.textContent,error:typeof snapshotError==="string"?snapshotError:null})'),
      calls.map(call => [call.name,call.result.isError && call.result.content]));
    throw error;
  }
  if (process.argv[4] === "--editor") {
    releaseRefresh();
    const read = async () => (await rpc("tools/call",{name:"board_snapshot",arguments:{}})).structuredContent;
    const report = await checkPackagedEditor({evaluate,command:outer.command},read);
    assert.deepEqual(calls.filter(call => call.name === "board_update").map(call => call.arguments.note),report.submitted);
    console.error(JSON.stringify({surface:"MCP",...report}));
  } else await checkArchivePanel(evaluate);
} finally {
  releaseWrite();
  releaseRefresh();
  if (browser) await browser.close();
  await new Promise(resolve => server.close(resolve));
  input.close();
  process.stdin.destroy();
}

async function checkArchivePanel(evaluate) {
  await evaluate(`byId("host-filter").value="all";byId("host-filter").dispatchEvent(new Event("change"));
    window.buttonFor=id=>document.querySelector('.card[data-id="'+CSS.escape(id)+'"] [data-action=archive]')`);
  const button = id => `buttonFor(${JSON.stringify(id)})`;
  const rows = calls.find(call => call.name === "board_snapshot").result.structuredContent.sessions;
  const cached = rows.find(row => row.hostId === "offline");
  await evaluate('byId("history-filter").value="cached";byId("history-filter").dispatchEvent(new Event("change"))');
  assert.deepEqual(await evaluate(`(() => {
    const row=document.querySelector('.card[data-id="'+CSS.escape(${JSON.stringify(cached.id)})+'"]');
    return [row.draggable,row.querySelector("[data-action=adjust]").disabled,Boolean(row.querySelector("[data-action=archive]"))];
  })()`), [false,true,false]);
  await evaluate('byId("history-filter").value="all";byId("history-filter").dispatchEvent(new Event("change"))');
  assert.equal(await evaluate(`${button(target)}.textContent`), "归档");
  assert.equal(await evaluate(`${button(follow)}.textContent`), "确认待归档");
  await evaluate(`${button(follow)}.closest(".card").querySelector(".card-title").click()`);
  await until(() => calls.some(call => call.name === "board_preview_reply") &&
    evaluate('!byId("reply-reader").hidden && byId("reply-body").getAttribute("aria-busy")==="false"'), "预览未完成");
  assert.equal(await evaluate('byId("reply-text").textContent.trim()'), "归档前核对",
    JSON.stringify(calls.find(call => call.name === "board_preview_reply").result));
  await evaluate('byId("reply-close").click()');
  await until(() => evaluate('byId("reply-reader").hidden'), "预览未收起");
  assert.equal(calls.some(call => ["board_archive","board_update"].includes(call.name)), false);
  const ready = rows.find(row => row.id === target), pending = rows.find(row => row.id === follow);
  await evaluate('document.querySelector("[data-archive-all]").click()');
  await until(() => calls.some(call => call.name === "board_archive"), "未收到归档工具调用");
  assert.equal(await evaluate('document.querySelector("[data-archive-all]").disabled && !document.querySelector(".card[draggable=true],.card [data-action=adjust]:enabled,.card [data-action=archive]:enabled")'), true);
  releaseWrite();
  await until(() => calls.some(call => call.name === "board_archive_batch_finish") && evaluate('!saving && !archiveChecking'), "未读回列归档");
  const submitted = calls.filter(call => call.name === "board_archive_batch_start");
  assert.equal(submitted.length, 1);
  const {id:batchId,items} = submitted[0].arguments;
  const revision = ({id,version,inputVersion,sectionVersion}) => ({id,version,inputVersion,sectionVersion});
  assert.deepEqual(items,[{...revision(ready),title:ready.title}]);
  assert.deepEqual(calls.filter(call => call.name === "board_archive").map(call => call.arguments), [{...revision(ready),batchId}]);
  await until(() => evaluate(`${button(follow)}?.disabled === false && !saving`), "确认按钮未恢复");
  await evaluate(`${button(follow)}.click()`);
  await until(() => evaluate(`${button(follow)}?.textContent === "归档" && !saving`), "新输入未确认");
  const updates = calls.filter(call => call.name === "board_update");
  assert.deepEqual(updates.map(call => call.arguments), [{...revision(pending),state:"to_archive",note:pending.note}]);
  const confirmed = updates[0].result.structuredContent.session;
  assert.equal(confirmed.followUp, false);
  assert.equal(confirmed.id, follow);
  assert.deepEqual(await evaluate(`data.sessions.find(row => row.id === ${JSON.stringify(follow)})`), confirmed);
  assert.equal(calls.filter(call => call.name === "board_archive").length, 1);
}
