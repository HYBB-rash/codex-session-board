import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import http from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
const bridge = await readFile(new URL("../assets/app-bridge.js", import.meta.url), "utf8");
const states = {in_progress:"进行中",review:"待审阅",blocked:"已阻塞",to_archive:"待归档"};
const sharedTitle = "核对发布及隔离启动历史记录".repeat(6);
const session = (id, state, title = sharedTitle, extra = {}) => ({
  id,state,title,version:0,inputVersion:0,sectionVersion:"fixture",updatedAt:1760000000,...extra,
});
const fixture = () => ({
  states,health:{},fourColumns:true,projectCatalog:{loaded:true,items:[]},
  sessions:[session("local","review"),session("remote:mac:session","in_progress",sharedTitle,{remote:true,hostId:"mac",hostLabel:"MacBook"}),
    session("empty","blocked","尚无回复"),session("offline","to_archive","读取失败",{canOpen:false}),
    ...Array.from({length:20},(_,index)=>session(`review-${index}`,"review",`核对记录 ${index}`))],
});
const markdown = "<h1>隔离启动</h1><ul><li>Linux</li></ul><blockquote>检查报告</blockquote><pre><code>printf ok</code></pre>" +
  '<table><tr><td>通过</td></tr></table><p><a href="https://example.com/report">报告</a></p>' +
  "<p>&lt;script&gt;示例&lt;/script&gt;</p>" + "<p>核对详情</p>".repeat(80);
const prior = {turnId:"old",inProgress:true,html:markdown};
const requests = [];
let snapshot = fixture(), pending, current, held, openFails;
const host = `<!doctype html><meta charset="utf-8">
<style>body{margin:0}iframe{display:block;border:0;width:100vw;height:100vh}</style>
<script>
const readRoutes={board_snapshot:"/api/sessions",board_archive_batches:"/api/archive-batches"};
const postRoutes={board_preview_reply:"/api/preview-reply",board_open:"/api/open"};
addEventListener("message",async event=>{
  if(event.source!==document.querySelector("iframe").contentWindow)return;
  const {method,id,params}=event.data;
  let result;
  if(method==="ui/initialize")result={protocolVersion:params.protocolVersion,hostInfo:{name:"preview-check",version:"1"},hostCapabilities:{openLinks:{}},hostContext:{}};
  if(method==="ui/open-link"){window.lastLink=params.url;result={};}
  if(method==="tools/call"){
    const readPath=readRoutes[params.name];
    const response=readPath ? await fetch(readPath) : await fetch(postRoutes[params.name],{
      method:"POST",body:JSON.stringify(params.arguments)
    });
    const value=await response.json();
    result=!response.ok?{isError:true,content:[{type:"text",text:value.error}]}:params.name==="board_preview_reply"?
      {content:[],_meta:{previewReply:value}}:{content:[],structuredContent:value};
  }
  if(result)event.source.postMessage({jsonrpc:"2.0",id,result},"*");
});
</script><iframe sandbox="allow-scripts" src="/app"></iframe>`;
const server = http.createServer(async (request,response) => {
  if (request.url === "/" || request.url === "/host" || request.url === "/app") {
    return response.writeHead(200,{"Content-Type":"text/html; charset=utf-8"}).end(
      request.url === "/host" ? host : request.url === "/app" ?
        html.replace("<script>",()=>`<script>${bridge.replaceAll("</script","<\\/script")}</script><script>`) : html);
  }
  response.setHeader("Content-Type","application/json");
  if (request.url === "/api/sessions") return response.end(JSON.stringify(snapshot));
  if (request.url === "/api/archive-batches" && request.method === "GET") {
    return response.end(JSON.stringify({batches:[]}));
  }
  if (request.method !== "POST") return response.writeHead(404).end("{}");
  let encoded = "";
  for await (const chunk of request) encoded += chunk;
  const {id} = JSON.parse(encoded);
  requests.push([request.url,id]);
  if (request.url === "/api/open") {
    return openFails ? response.writeHead(503).end('{"error":"无法打开原会话"}') : response.end('{"ok":true}');
  }
  if (request.url !== "/api/preview-reply" || id === "offline") {
    return response.writeHead(503).end('{"error":"未连接"}');
  }
  if (id === "local" && held) {pending = response;return;}
  const reply = id === "empty" ? {turnId:null,html:null,inProgress:false} : id === "local" ?
    {turnId:"local",html:"<p>本机内容</p>",inProgress:false} : current;
  response.end(JSON.stringify(reply));
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
let browser, cleanup;
const checks = [];
const address = `http://127.0.0.1:${server.address().port}`;
const until = async (predicate, message) => {
  for (let attempt=0;attempt<150;attempt++) {
    const value = await predicate();
    if (value) return value;
    await delay(20);
  }
  assert.fail(message);
};
const card = id => `.card[data-id="${id}"] .card-title`;
const remote = "remote:mac:session";
const overlayClosed = 'byId("reply-reader").hidden && byId("reply-scrim").hidden';
const geometry = `(() => {
  const bounds = node => node.getBoundingClientRect().toJSON();
  return {
    columns:[...document.querySelectorAll(".column")].map(bounds),
    titles:[...document.querySelectorAll(".card-title")].map(bounds),
    lists:[...document.querySelectorAll(".column .cards")].map(list=>list.scrollTop),
    horizontal:byId("board-viewport").scrollLeft, filter:byId("host-filter").value,
  };
})()`;
const center = selector => `(() => {
  const bounds = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();
  return {x:bounds.left+bounds.width/2,y:bounds.top+bounds.height/2};
})()`;
const release = () => {
  pending.end(JSON.stringify({turnId:"local",html:"<p>本机迟到内容</p>",inProgress:false}));
  pending = null;
};
async function iframeEvaluator() {
  const child = await until(async ()=>(await browser.command("Target.getTargets")).targetInfos
    .find(target=>target.type==="iframe" && target.url===address+"/app"),"未找到隔离插件 iframe");
  const {sessionId} = await browser.command("Target.attachToTarget",{targetId:child.targetId,flatten:true});
  return async expression => {
    const result = await browser.command("Runtime.evaluate",{expression,returnByValue:true,awaitPromise:true},sessionId);
    assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
}
async function exercise(surface) {
  snapshot = fixture(); current = prior; held = true; openFails = false; requests.length = 0;
  const page = await browser.page(address+(surface==="mcp" ? "/host" : "/"));
  const evaluate = surface==="mcp" ? await iframeEvaluator() : page.evaluate;
  const wait = expression => until(()=>evaluate(expression),expression);
  const resize = async (width,height) => {
    await page.command("Emulation.setDeviceMetricsOverride",{width,height,deviceScaleFactor:1,mobile:false});
    await wait(`innerWidth === ${width} && innerHeight === ${height}`);
  };
  const press = selector => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
  const mouse = (type, point, buttons) => page.command("Input.dispatchMouseEvent",{
    type,...point,button:"left",buttons,clickCount:1,
  });
  const click = async point => {
    await mouse("mousePressed",point,1);
    await mouse("mouseReleased",point,0);
  };
  const gutter = () => evaluate(`(() => {
    const {right,top,height}=byId("reply-reader").getBoundingClientRect();
    return {x:Math.floor(right)+4,y:Math.floor(top+height/2)};
  })()`);
  await wait(`Boolean(document.querySelector(${JSON.stringify(card("local"))}))`);
  await evaluate(`(() => {
    const readReply = openReply;
    window.settledLocalReads = 0;
    openReply = async (session, focus) => {
      await readReply(session, focus);
      if (session.id === "local") window.settledLocalReads++;
    };
  })()`);
  await evaluate('byId("host-filter").value="all";byId("host-filter").dispatchEvent(new Event("change"))');
  await evaluate(`document.querySelector(${JSON.stringify(card("local"))}).scrollIntoView({block:"nearest"})`);
  const baseline = await evaluate(geometry);
  await click(await evaluate(center(card("local"))));
  await until(()=>pending,"本机请求未被延迟");
  assert.equal(await evaluate('byId("reply-scrim").hidden'),false);
  assert.deepEqual(await evaluate(geometry),baseline);
  await click(await evaluate(center(card(remote))));
  await wait('byId("reply-status").textContent === "上一条完整回复 · 当前轮次进行中"');
  assert.deepEqual(await evaluate(geometry),baseline);
  release();
  await wait("window.settledLocalReads === 1");
  await wait('byId("reply-text").innerHTML.includes("隔离启动")');
  const elements = '["h1","li","blockquote","pre code","table","a[href]"]';
  assert(await evaluate(`${elements}.every(tag=>byId("reply-text").querySelector(tag)) &&
    !byId("reply-text").querySelector("script") && byId("reply-meta").textContent.includes("MacBook")`));
  if (surface==="mcp") {
    await press("#reply-text a");
    await until(()=>page.evaluate('window.lastLink === "https://example.com/report"'),"插件未通过宿主打开链接");
  }
  const start = await evaluate(center("#reply-text h1"));
  const outside = await evaluate(`({x:byId("reply-reader").getBoundingClientRect().left-7,
    y:byId("reply-text").querySelector("h1").getBoundingClientRect().bottom+8})`);
  await mouse("mousePressed",start,1);
  await mouse("mouseMoved",outside,1);
  await mouse("mouseReleased",outside,0);
  assert(await evaluate('!byId("reply-reader").hidden && getSelection().toString().trim().length > 0'));
  await evaluate("getSelection().removeAllRanges()");
  if (process.argv[2]) {
    const capture = await page.command("Page.captureScreenshot",{format:"png"});
    await writeFile(`${process.argv[2]}/${surface}.png`,Buffer.from(capture.data,"base64"));
  }
  for (const [width,height] of [[768,431],[768,430],[390,600],[390,320],[320,361],[320,256]]) {
    await resize(width,height);
    const layout = await evaluate(`(() => {
      const reader=byId("reply-reader"), body=byId("reply-body"), open=byId("reply-open");
      open.focus({preventScroll:true});
      return {bodyHeight:body.clientHeight, bodyScrollHeight:body.scrollHeight, openBottom:open.getBoundingClientRect().bottom,
        readerBottom:reader.getBoundingClientRect().bottom, focused:document.activeElement === open,
        fullTitle:byId("reply-title").title};
    })()`);
    assert(layout.bodyHeight >= 96 && layout.bodyScrollHeight > layout.bodyHeight &&
      layout.openBottom <= Math.min(height,layout.readerBottom) && layout.focused && layout.fullTitle === sharedTitle,
    `阅读栏在 ${width}×${height} 中裁切正文或原会话入口 ${JSON.stringify(layout)}`);
  }
  await resize(768,431);
  const overlayBounds = `["reply-reader","reply-scrim"].map(id=>byId(id).getBoundingClientRect().toJSON())`;
  const beforeWheel = await evaluate(overlayBounds);
  await page.command("Input.dispatchMouseEvent",{type:"mouseWheel",x:8,y:250,deltaX:0,deltaY:480});
  await wait("window.scrollY > 0");
  assert.deepEqual(await evaluate(overlayBounds),beforeWheel);
  assert.equal(beforeWheel[1].bottom,431);
  await evaluate("window.scrollTo(0,0)");
  await resize(1680,1000);
  const scroll = await evaluate('byId("reply-body").scrollTop=140;byId("reply-body").scrollTop');
  assert(scroll>0);
  snapshot.sessions[1].previewTurnId = "new";
  current = {turnId:"new",html:"<p>本轮已完成</p>",inProgress:false};
  await evaluate("load()");
  await wait('byId("reply-refresh").textContent === "查看新答复"');
  assert.equal(await evaluate('byId("reply-body").scrollTop'),scroll);
  await press("#reply-refresh");
  await wait('byId("reply-text").textContent === "本轮已完成"');
  await evaluate('byId("host-filter").focus({preventScroll:true})');
  await page.command("Input.dispatchKeyEvent",{type:"keyDown",key:"Escape",code:"Escape",windowsVirtualKeyCode:27});
  await wait(`${overlayClosed} && document.activeElement.matches(${JSON.stringify(card(remote))})`);
  await press(card("local"));
  await until(()=>pending,"预览请求未被延迟");
  await click(await gutter());
  await wait(overlayClosed);
  release();
  await wait("window.settledLocalReads === 2");
  assert(await evaluate(`${overlayClosed} && !byId("reply-text").textContent`),"迟到正文改变了收起状态");
  await press(card("empty"));
  await wait('byId("reply-status").textContent === "还没有完整回复" && !byId("reply-open").disabled');
  await press(card("offline"));
  await wait('byId("reply-status").classList.contains("error") && byId("reply-open").disabled');
  await press(card(remote));
  await wait('byId("reply-text").textContent === "本轮已完成"');
  openFails = true;
  await press("#reply-open");
  await wait('byId("feedback").textContent === "无法打开原会话" && !opening');
  assert(await evaluate('!byId("reply-reader").hidden && byId("reply-text").textContent === "本轮已完成"'));
  openFails = false;
  await press("#reply-open");
  await wait('byId("reply-reader").hidden && !opening');
  assert.deepEqual(requests.filter(([path])=>path!=="/api/preview-reply"),[["/api/open",remote],["/api/open",remote]]);
  const position = await evaluate('const list=byId("column-review").querySelector(".cards");list.scrollTop=90;list.scrollTop');
  assert(position>0);
  const scrolled = await evaluate(geometry);
  for (const blank of ["#column-review .column-heading h2","#column-in_progress .cards"]) {
    await click(await evaluate(center(card(remote))));
    await wait('!byId("reply-scrim").hidden');
    await click(await evaluate(center(blank)));
    await wait(overlayClosed);
    assert.deepEqual(await evaluate(geometry),scrolled);
  }
  await click(await evaluate(center(card(remote))));
  await wait('byId("reply-text").textContent === "本轮已完成"');
  const beforeAdjust = requests.length;
  await click(await evaluate(center(`.card[data-id="${remote}"] [data-action="adjust"]`)));
  await wait('byId("editor").open && !byId("reply-scrim").hidden');
  assert.equal(requests.length,beforeAdjust);
  await evaluate('byId("editor").close()');
  await click(await gutter());
  await wait(overlayClosed);
  await resize(768,900);
  const horizontal = await evaluate('byId("board-viewport").scrollLeft=10000;byId("board-viewport").scrollLeft');
  assert(horizontal>0);
  const narrow = await evaluate(geometry);
  await press(card(remote));
  assert.deepEqual(await evaluate(geometry),narrow);
  await click(await gutter());
  await wait(overlayClosed);
  assert.deepEqual(await evaluate(geometry),narrow);
  if (surface==="http") await checkTitleDrag(page, evaluate);
  checks.push(surface);
}
async function checkTitleDrag(page, evaluate) {
  await page.command("Emulation.setDeviceMetricsOverride",{width:1680,height:1000,deviceScaleFactor:1,mobile:false});
  await evaluate(`document.querySelector(${JSON.stringify(card("local"))}).scrollIntoView({block:"center"})`);
  const source = await evaluate(center(card("local"))), target = await evaluate(center("#column-blocked .cards"));
  const before = requests.length;
  await page.command("Input.setInterceptDrags",{enabled:true});
  const drag = page.event("Input.dragIntercepted");
  await page.command("Input.dispatchMouseEvent",{type:"mouseMoved",...source});
  await page.command("Input.dispatchMouseEvent",{type:"mousePressed",button:"left",buttons:1,clickCount:1,...source});
  await page.command("Input.dispatchMouseEvent",{type:"mouseMoved",button:"left",buttons:1,x:source.x+12,y:source.y+12});
  await page.command("Input.dispatchMouseEvent",{type:"mouseMoved",button:"left",buttons:1,...target});
  const {data} = await drag;
  for (const type of ["dragEnter","dragOver","drop"]) await page.command("Input.dispatchDragEvent",{type,data,...target});
  await page.command("Input.dispatchMouseEvent",{type:"mouseReleased",button:"left",clickCount:1,...target});
  await page.command("Input.setInterceptDrags",{enabled:false});
  assert(await evaluate('byId("editor").open && byId("reply-reader").hidden'));
  assert.equal(requests.length,before);
}
try {
  browser = await launchBrowser();
  await exercise("http");
  await exercise("mcp");
} finally {
  pending?.end();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve=>server.close(resolve));
}
console.log(JSON.stringify({passed:true,checks,cleanup}));
