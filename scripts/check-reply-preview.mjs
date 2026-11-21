import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import http from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";
import { checkCardDisplay } from "./check-card-display.mjs";

const html = await readFile(process.argv[4] || new URL("../index.html", import.meta.url), "utf8");
const bridge = await readFile(new URL("../assets/app-bridge.js", import.meta.url), "utf8");
const embedded = process.argv[3] ? await readFile(process.argv[3], "utf8") :
  html.replace("<script>",()=>`<script>${bridge.replaceAll("</script","<\\/script")}</script><script>`);
const states = {in_progress:"进行中",review:"待审阅",blocked:"已阻塞",to_archive:"待归档"};
const sharedTitle = "核对发布及隔离启动历史记录".repeat(6);
const session = (id, state, title = sharedTitle, extra = {}) => ({
  id,state,title,version:0,inputVersion:0,sectionVersion:"fixture",updatedAt:1760000000,readOnly:false,canArchive:true,...extra,
});
const fixture = () => ({
  states,health:{},fourColumns:true,projectCatalog:{loaded:true,items:[]},
  sessions:[session("local","review"),session("remote:mac:session","in_progress",sharedTitle,{remote:true,hostId:"mac",hostLabel:"MacBook"}),
    session("empty","blocked","尚无回复"),session("offline","to_archive","读取失败",{canOpen:false,readOnly:true,canArchive:false}),
    ...Array.from({length:20},(_,index)=>session(`review-${index}`,"review",`核对记录 ${index}`))],
});
const markdown = "<h1>隔离启动</h1><ul><li>Linux</li></ul><blockquote>检查报告</blockquote><pre><code>printf ok</code></pre>" +
  '<table><tr><td>通过</td></tr></table><p><a href="https://example.com/report">报告</a></p>' +
  "<p>&lt;script&gt;示例&lt;/script&gt;</p>" + "<p>核对详情</p>".repeat(80);
const prior = {turnId:"old",inProgress:true,html:markdown};
const requests = [];
const blockedResources = [];
let snapshot = fixture(), pending, current, held, openFails;
const host = `<!doctype html><meta charset="utf-8">
<style>body{margin:0}iframe{position:fixed;top:48px;left:0;border:0;width:100vw;height:calc(100vh - 48px)}</style>
<script>
const readRoutes={board_snapshot:"/api/sessions",board_archive_batches:"/api/archive-batches",
  board_plugin_update_status:"/api/plugin-update"};
const postRoutes={board_preview_reply:"/api/preview-reply",board_open:"/api/open"};
window.creations=[];
addEventListener("message",async event=>{
  if(event.source!==document.querySelector("iframe").contentWindow)return;
  const {method,id,params}=event.data;
  let result;
  if(method==="ui/initialize")result={protocolVersion:params.protocolVersion,hostInfo:{name:"preview-check",version:"1"},hostCapabilities:{openLinks:{}},hostContext:{theme:"dark"}};
  if(method==="ui/open-link"){window.lastLink=params.url;result={};}
  if(method==="tools/call" && params.name==="board_new"){
    window.creations.push(params.arguments);
    result={content:[],structuredContent:{ok:true}};
  }else if(method==="tools/call"){
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
window.notifyTheme=theme=>document.querySelector("iframe").contentWindow.postMessage({
  jsonrpc:"2.0",method:"ui/notifications/host-context-changed",params:{theme}
},"*");
</script><iframe sandbox="allow-scripts" src="/app"></iframe>`;
const server = http.createServer(async (request,response) => {
  if (request.url === "/resource-policy-violation") {
    let report = "";
    for await (const chunk of request) report += chunk;
    blockedResources.push(JSON.parse(report)["csp-report"]);
    return response.writeHead(204).end();
  }
  if (request.url === "/" || request.url === "/host" || request.url === "/app") {
    if (request.url === "/app") response.setHeader("Content-Security-Policy",
      "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' data:; style-src 'unsafe-inline' data:; img-src data:; font-src data:; report-uri /resource-policy-violation");
    return response.writeHead(200,{"Content-Type":"text/html; charset=utf-8"}).end(
      request.url === "/host" ? host : request.url === "/app" ? embedded : html);
  }
  response.setHeader("Content-Type","application/json");
  if (request.url === "/api/sessions") return response.end(JSON.stringify(snapshot));
  if (request.url === "/api/plugin-update" && request.method === "GET") {
    return response.end(JSON.stringify({available:false}));
  }
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
let browser, cleanup, failure;
const checks = [];
const firstClicks = {};
const gutterClicks = {http:[],mcp:[]};
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
  try {
    await exercisePage(surface,page);
  } finally {
    await page.command("Page.close");
  }
}
async function exercisePage(surface, page) {
  const evaluate = surface==="mcp" ? await iframeEvaluator() : page.evaluate;
  const wait = expression => until(()=>evaluate(expression),expression);
  const hostOffset = () => surface === "http" ? {x:0,y:0} : page.evaluate(`(() => {
    const frame = document.querySelector("iframe"), bounds = frame.getBoundingClientRect();
    return {x:bounds.left+frame.clientLeft,y:bounds.top+frame.clientTop};
  })()`);
  const pagePoint = async point => {
    const offset = await hostOffset();
    return {x:offset.x+point.x,y:offset.y+point.y};
  };
  const resize = async (width,height) => {
    const offset = await hostOffset();
    await page.command("Emulation.setDeviceMetricsOverride",{width,height:height+offset.y,deviceScaleFactor:1,mobile:false});
    await wait(`innerWidth === ${width} && innerHeight === ${height}`);
  };
  const press = selector => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
  const mouse = async (type, point, buttons) => page.command("Input.dispatchMouseEvent",{
    type,...await pagePoint(point),button:buttons || type==="mouseReleased" ? "left" : "none",buttons,clickCount:1,
  });
  const click = async point => {
    await mouse("mousePressed",point,1);
    await mouse("mouseReleased",point,0);
  };
  const dismissFromGutter = async () => {
    await wait('!byId("reply-reader").hidden && !byId("reply-scrim").hidden');
    const startTrace = `window.gutterEvents = []; window.gutterMove = null; window.gutterTrace = new AbortController();
      for (const type of ["mousemove","pointerdown","pointerup","click"]) window.addEventListener(type,event=>{
        const hit = {type,target:event.target.tagName,id:event.target.id,card:event.target.closest(".card")?.dataset.id,
          blank:event.target === document.querySelector("main"),trusted:event.isTrusted,x:event.clientX,y:event.clientY};
        if (type === "pointerdown" && document.getElementById("reply-reader"))
          hit.previewOpen = !document.getElementById("reply-reader").hidden && !document.getElementById("reply-scrim").hidden;
        if (type === "mousemove") window.gutterMove = hit;
        else window.gutterEvents.push(hit);
      },{capture:true,signal:window.gutterTrace.signal})`;
    const endTrace = "window.gutterTrace.abort(); window.gutterEvents";
    await evaluate(startTrace);
    if (surface === "mcp") await page.evaluate(startTrace);
    const point = await until(async () => {
      const blank = await evaluate(`(() => {
        const reader = byId("reply-reader").getBoundingClientRect();
        const left = Math.max(reader.right, byId("board-viewport").getBoundingClientRect().right);
        const right = document.documentElement.clientWidth;
        const x = Math.floor((left + right) / 2), y = Math.floor((reader.top + reader.bottom) / 2);
        return x > left && x < right && document.elementFromPoint(x,y) === document.querySelector("main") && {x,y};
      })()`);
      if (!blank) return false;
      await mouse("mouseMoved",blank,0);
      const hit = await evaluate("window.gutterMove");
      return hit?.trusted && hit.blank && hit.x===blank.x && hit.y===blank.y && blank;
    },`${surface} 物理指针未命中阅读栏右侧的页面空白`);
    await click(point);
    const events = await evaluate(endTrace);
    const hostEvents = surface === "mcp" ? await page.evaluate(endTrace) : [];
    const receipt = {point,page:await pagePoint(point),events,hostEvents};
    gutterClicks[surface].push(receipt);
    const detail = JSON.stringify(receipt);
    assert.deepEqual(events.filter(event => event.blank && event.trusted && event.x===point.x && event.y===point.y)
      .map(event => event.type), ["pointerdown","pointerup","click"], `${surface} 空白未收到完整物理点击 ${detail}`);
    assert.equal(events.find(event => event.type==="pointerdown" && event.trusted).previewOpen,true,
      `${surface} 物理按下前预览已收起 ${detail}`);
    await until(()=>evaluate(overlayClosed),`${surface} 页面空白收到点击后预览未收起 ${detail}`);
  };
  await wait(`Boolean(document.querySelector(${JSON.stringify(card("local"))}))`);
  if (surface === "mcp") {
    const scheme = 'getComputedStyle(document.documentElement).colorScheme';
    const background = 'getComputedStyle(document.body).backgroundColor';
    assert.equal(await evaluate(scheme),"dark");
    const darkBackground = await evaluate(background);
    await page.evaluate('notifyTheme("light")');
    await wait(`${scheme} === "light"`);
    assert.notEqual(await evaluate(background),darkBackground,"嵌入看板背景没有跟随宿主主题");
    assert.equal(await evaluate('document.querySelector("iframe,frame,link[rel~=manifest]") === null'),true);
    assert.equal(await evaluate(`(async () => {
      const image = document.querySelector(".dragon-scene img");
      await image.decode();
      return image.currentSrc.startsWith("data:image/") && image.naturalWidth > 0;
    })()`),true);
    await evaluate('byId("host-filter").value="local";byId("host-filter").dispatchEvent(new Event("change"))');
    const position = await evaluate(`(() => {
      const button = [...document.querySelectorAll("button")].find(node => node.textContent.includes("新建会话"));
      const box = button.getBoundingClientRect();
      return {x:box.left+box.width/2,y:box.top+box.height/2};
    })()`);
    assert.deepEqual(await page.evaluate("window.creations"),[],"点击前宿主已收到新建请求");
    await click(position);
    await until(()=>page.evaluate("window.creations.length > 0"),"点击新建后宿主没有收到 board_new");
  }
  await evaluate(`(() => {
    window.settledLocalReads = 0;
    const local = (path, body) => path === "/api/preview-reply" && body?.id === "local";
    if (window.boardRequest) {
      const request = window.boardRequest;
      window.boardRequest = async (path, body) => {
        try {return await request(path, body);}
        finally {if (local(path, body)) window.settledLocalReads++;}
      };
    } else {
      const fetch = window.fetch;
      window.fetch = async (path, options) => {
        const response = await fetch(path, options);
        if (!local(path, options?.body && JSON.parse(options.body))) return response;
        const json = response.json.bind(response);
        response.json = async () => {
          try {return await json();}
          finally {window.settledLocalReads++;}
        };
        return response;
      };
    }
  })()`);
  await evaluate('byId("host-filter").value="all";byId("host-filter").dispatchEvent(new Event("change"))');
  await evaluate(`document.querySelector(${JSON.stringify(card("local"))}).scrollIntoView({block:"nearest"})`);
  const baseline = await evaluate(geometry);
  const expanded = () => evaluate('Array.from(document.querySelectorAll(".card-title[aria-expanded=true]"),title=>title.closest(".card").dataset.id)');
  await evaluate(`window.titleWrites = [];
    window.titleObserver = new MutationObserver(records => {
      for (const {target} of records) if (target.matches(".card-title"))
        titleWrites.push(target.closest(".card").dataset.id);
    });
    titleObserver.observe(byId("board"),{subtree:true,attributes:true,attributeFilter:["aria-expanded"]})`);
  const selector = JSON.stringify(card("local"));
  const point = await until(async () => {
    const visible = await evaluate(`(() => {
      const title = document.querySelector(${selector}), bounds = title.getBoundingClientRect();
      const list = title.closest(".cards").getBoundingClientRect();
      const left = Math.max(bounds.left,list.left,0), right = Math.min(bounds.right,list.right,innerWidth);
      const top = Math.max(bounds.top,list.top,0), bottom = Math.min(bounds.bottom,list.bottom,innerHeight);
      const x = (left+right)/2, y = (top+bottom)/2;
      return right>left && bottom>top && title.contains(document.elementFromPoint(x,y)) && {x,y};
    })()`);
    if (!visible) return false;
    await mouse("mouseMoved",visible,0);
    return await evaluate(`document.querySelector(${selector}).matches(":hover")`) && visible;
  },"物理指针未命中可见的本机卡标题");
  await evaluate(`addEventListener("click",event=>{
    window.previewClick={trusted:event.isTrusted,card:event.target.closest(".card")?.dataset.id,
      title:Boolean(event.target.closest(".card-title")),x:event.clientX,y:event.clientY};
  },{capture:true,once:true})`);
  await click(point);
  const hit = await until(()=>evaluate("window.previewClick || null"),`${surface} 在 ${JSON.stringify(point)} 派发首击后未收到点击事件`);
  firstClicks[surface] = {page:await pagePoint(point),hit};
  assert(hit.trusted && hit.card==="local" && hit.title,`${surface} 首击未命中本机卡标题 ${JSON.stringify(firstClicks[surface])}`);
  await until(()=>pending,`${surface} 本机卡标题收到首击，未收到预览请求 ${JSON.stringify(hit)}`);
  assert.equal(await evaluate('byId("reply-scrim").hidden'),false);
  assert.deepEqual(await expanded(),["local"]);
  assert.deepEqual(await evaluate("titleWrites.splice(0)"),["local"]);
  assert.deepEqual(await evaluate(geometry),baseline);
  await click(await evaluate(center(card(remote))));
  await wait('byId("reply-status").textContent === "上一条完整回复 · 当前轮次进行中"');
  assert.deepEqual(await expanded(),[remote]);
  assert.deepEqual(await evaluate("titleWrites.splice(0).sort()"),["local",remote].sort());
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
  await page.command("Input.dispatchMouseEvent",{type:"mouseWheel",...await pagePoint({x:8,y:250}),deltaX:0,deltaY:480});
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
  assert.deepEqual(await expanded(),[remote]);
  assert(await evaluate('byId("reply-text").innerHTML.includes("隔离启动")'));
  assert.equal(await evaluate('byId("reply-body").scrollTop'),scroll);
  const readingState = `({html:byId("reply-text").innerHTML,scroll:byId("reply-body").scrollTop,
    busy:byId("reply-body").getAttribute("aria-busy"),
    selected:document.querySelector(".card-title[aria-expanded=true]").closest(".card").dataset.id})`;
  const preserved = await evaluate(readingState), beforeRefresh = requests.length;
  const dragEvent = type => `document.querySelector(".card").dispatchEvent(new DragEvent("${type}",{bubbles:true,dataTransfer:new DataTransfer()}))`;
  for (const operation of [dragEvent("dragstart"), dragEvent("dragend"), "saving=true", "creating=true",
    `moving={id:${JSON.stringify(remote)}}`]) {
    await evaluate(operation);
    await press("#reply-refresh");
    await evaluate("new Promise(resolve=>requestAnimationFrame(resolve))");
    assert.equal(requests.length,beforeRefresh,`${surface} ${operation} 期间重读了回复`);
    assert.deepEqual(await evaluate(readingState),preserved);
    await evaluate("saving=false;creating=false;moving=null");
    if (operation === dragEvent("dragend")) await delay(410);
  }
  await evaluate('saving=true;moving={id:"another-card"}');
  await press("#reply-refresh");
  await wait('byId("reply-text").textContent === "本轮已完成"');
  assert.equal(requests.length,beforeRefresh+1);
  await evaluate("saving=false;moving=null");
  await evaluate('byId("host-filter").focus({preventScroll:true})');
  await evaluate("titleWrites.length=0");
  await page.command("Input.dispatchKeyEvent",{type:"keyDown",key:"Escape",code:"Escape",windowsVirtualKeyCode:27});
  await wait(`${overlayClosed} && document.activeElement.matches(${JSON.stringify(card(remote))})`);
  assert.deepEqual(await expanded(),[]);
  assert.deepEqual(await evaluate("titleObserver.disconnect();titleWrites"),[remote]);
  await press(card("local"));
  await until(()=>pending,"预览请求未被延迟");
  const blankHeading = "#column-review .column-heading h2";
  const outsidePoint = await evaluate(center(blankHeading));
  const hitHeading = await evaluate(`Boolean(document.elementFromPoint(${outsidePoint.x},${outsidePoint.y})
    ?.closest(${JSON.stringify(blankHeading)}))`);
  assert(hitHeading, `${surface} 点击点未命中预览栏外的列标题：${JSON.stringify(outsidePoint)}`);
  await click(outsidePoint);
  await until(()=>evaluate(overlayClosed), `${surface} 点击预览栏外的列标题后未收起：${JSON.stringify(outsidePoint)}`);
  release();
  await wait("window.settledLocalReads === 2");
  assert(await evaluate(`${overlayClosed} && !byId("reply-text").textContent`),"迟到正文改变了收起状态");
  await press(card("empty"));
  await wait('byId("reply-status").textContent === "还没有完整回复" && !byId("reply-open").disabled');
  await press(card("offline"));
  await wait('byId("reply-status").classList.contains("error") && byId("reply-open").disabled');
  await evaluate('byId("reply-close").focus({preventScroll:true})');
  snapshot.sessions = snapshot.sessions.filter(row => row.id !== "offline");
  await evaluate("load()");
  await press("#reply-close");
  await wait(`${overlayClosed} && document.activeElement === byId("host-filter")`);
  assert.deepEqual(await expanded(),[]);
  await press(card(remote));
  await wait('byId("reply-text").textContent === "本轮已完成"');
  openFails = true;
  await press("#reply-open");
  await wait('byId("feedback").textContent === "无法打开原会话" && !sessionReader.opening');
  assert(await evaluate('!byId("reply-reader").hidden && byId("reply-text").textContent === "本轮已完成"'));
  openFails = false;
  await press("#reply-open");
  await wait('byId("reply-reader").hidden && !sessionReader.opening');
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
  await dismissFromGutter();
  await resize(768,900);
  const horizontal = await evaluate('byId("board-viewport").scrollLeft=10000;byId("board-viewport").scrollLeft');
  assert(horizontal>0);
  const narrow = await evaluate(geometry);
  await press(card(remote));
  assert.deepEqual(await evaluate(geometry),narrow);
  await dismissFromGutter();
  assert.deepEqual(await evaluate(geometry),narrow);
  await checkCardDisplay(evaluate, snapshot, wait, async selector => {
    await evaluate(`document.querySelector(${JSON.stringify(selector)}).scrollIntoView({block:"center",inline:"nearest",behavior:"instant"})`);
    await click(await evaluate(center(selector)));
  });
  if (surface==="http") await checkTitleDrag(page, evaluate);
  if (surface === "mcp") assert.deepEqual(await page.evaluate("window.creations"),[{}]);
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
  assert.deepEqual(blockedResources,[],"嵌入看板请求了包外资源");
} catch (error) {
  failure = error;
} finally {
  pending?.end();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve=>server.close(resolve));
}
console.log(JSON.stringify({passed:!failure,checks,firstClicks,gutterClicks,embeddedResource:process.argv[3] || "fixture",cleanup}));
if (failure) throw failure;
