import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";

const html = await fs.readFile(new URL("../index.html",import.meta.url),"utf8");
const fixture = {
  states:{unknown:"待判断",in_progress:"进行中",review:"待审阅",blocked:"已阻塞",to_archive:"待归档"},
  health:{lastSuccessAt:1760000000,scanComplete:true,runtimeVerified:true,nativeSections:true,nativeVerified:true},
  projectCatalog:{loaded:true,items:[{id:"local",label:"项目",paths:["/fixture"]}]},
  sessions:["long","short","readonly","warning"].map((id,index) => ({
    id,title:"会话 "+id,projectId:"local",cwd:"/fixture",state:index === 3 ? "blocked" : "review",
    note:index === 0 ? "完整备注".repeat(500) : index === 1 ? "短备注" : "",updatedAt:1760000000 + index,
    reason:index === 3 ? "必须始终显示的阻塞原因" : "",source:"codex",version:1,inputVersion:2,
    sectionVersion:"fixture",readOnly:index === 2,canArchive:index !== 2,
  })),
};
const writes = [], requests = [];
let failedPlacement = false;
const server = http.createServer(async (request,response) => {
  requests.push({method:request.method,path:request.url});
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST") {
    const input = JSON.parse(body);
    writes.push({path:request.url,body:input});
    if (request.url === "/api/state" && input.id === "unplaced") {
      if (failedPlacement) return response.writeHead(409,{"Content-Type":"application/json"})
        .end('{"error":"原生分区写入未确认"}');
      Object.assign(fixture.sessions.find(row => row.id === input.id),{
        state:input.state,note:input.note,version:input.version+1,sectionVersion:"verified-review",placementReason:null
      });
      return response.writeHead(200,{"Content-Type":"application/json"})
        .end('{"ok":true,"sectionVersion":"verified-review"}');
    }
    return response.writeHead(200,{"Content-Type":"application/json"}).end('{"ok":true}');
  }
  response.writeHead(200,{"Content-Type":request.url === "/" ? "text/html; charset=utf-8" : "application/json"});
  response.end(request.url === "/" ? html : JSON.stringify(fixture));
});
await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
let browser, cases = 0, baselineCases, cleanup, refreshPerformance;
const check = (actual,expected,message) => {assert.deepEqual(actual,expected,message);cases++;};
try {
  browser = await launchBrowser();
  const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
  const evaluate = expression => page.evaluate(expression);
  const wait = async expression => {
    for (let attempt = 0; attempt < 100; attempt++) {
      if (await evaluate(expression)) return;
      await new Promise(resolve => setTimeout(resolve,25));
    }
    assert.fail("界面状态未就绪："+expression);
  };
  const refresh = async () => {
    const expected = JSON.stringify(fixture);
    await evaluate("load()");
    await wait(`snapshotError === null && JSON.stringify(data) === ${JSON.stringify(expected)}`);
  };
  await wait('typeof data !== "undefined" && data.sessions.length === 4');
  await evaluate('window.scrollbarStyle=document.createElement("style");document.head.append(scrollbarStyle)');
  for (const scrollbarSize of [0,16,32]) {
    await evaluate(`scrollbarStyle.textContent=${JSON.stringify(scrollbarSize ?
      `::-webkit-scrollbar{width:${scrollbarSize}px;height:${scrollbarSize}px}` : "")}`);
    for (const [width,height] of [[320,256],[390,320],[1280,800]]) {
      await page.command("Emulation.setDeviceMetricsOverride",{width,height,deviceScaleFactor:1,mobile:false});
      await evaluate("new Promise(requestAnimationFrame)");
      const viewport = await evaluate('({client:byId("board-viewport").clientHeight,outer:byId("board-viewport").offsetHeight})');
      check(viewport.client >= 280,true,JSON.stringify({width,height,scrollbarSize,viewport}));
      if (scrollbarSize) check(viewport.outer - viewport.client,scrollbarSize);
      check(await evaluate('document.documentElement.scrollWidth <= innerWidth'),true);
      if (height < 400) check(await evaluate('document.documentElement.scrollHeight > innerHeight'),true);
    }
  }
  await evaluate("scrollbarStyle.remove()");
  const card = id => `document.querySelector('.card[data-id="${id}"]')`;
  await wait(`${card("long")}.querySelector(".note-details").classList.contains("expandable")`);
  check(await evaluate(`${card("long")}.querySelector(".note-preview").clientHeight <= 54`),true);
  check(await evaluate(`${card("long")}.getBoundingClientRect().height < 300`),true);
  check(await evaluate(`${card("short")}.querySelector(".note-details").classList.contains("expandable")`),false);
  check(await evaluate(`${card("short")}.querySelector(".note-details summary").tabIndex`),-1);
  check(await evaluate(`${card("warning")}.textContent.includes("必须始终显示的阻塞原因")`),true);
  await evaluate(`${card("long")}.querySelector(".note-details summary").click()`);
  check(await evaluate(`${card("long")}.querySelector(".note-details").open`),true);
  check(await evaluate(`${card("long")}.querySelector(".note-full").textContent`),fixture.sessions[0].note);
  await evaluate(`${card("long")}.querySelector(".project-tag").open=true;
    ${card("long")}.querySelector('[data-action="adjust"]').focus();window.originalCard=${card("long")};
    window.originalJump=byId("jump").children[2];window.originalWarning=${card("warning")};
    for(let cycle=0;cycle<20;cycle++){data=structuredClone(data);render();}`);
  check(await evaluate(`originalCard===${card("long")}`),true);
  check(await evaluate('document.activeElement.dataset.action'),"adjust");
  check(await evaluate('originalJump===byId("jump").children[2]'),true);
  check(await evaluate(`${card("long")}.querySelector(".note-details").open`),true);
  fixture.sessions.find(row=>row.id === "long").title = "更新后的会话";
  await refresh();
  check(await evaluate(`originalCard===${card("long")}`),false);
  check(await evaluate('document.activeElement.dataset.action'),"adjust");
  check(await evaluate('document.activeElement.closest(".card").dataset.id'),"long");
  check(await evaluate(`originalWarning===${card("warning")}`),true);
  check(await evaluate(`${card("long")}.querySelector(".project-tag").open`),true);
  check(await evaluate(`${card("long")}.querySelector(".note-details").open`),true);
  await evaluate('byId("jump").children[2].focus();render()');
  check(await evaluate('document.activeElement===originalJump'),true);
  await evaluate(`${card("long")}.querySelector('[data-action="adjust"]').click()`);
  check(await evaluate('byId("editor").open'),true);
  check(await evaluate('byId("note").value'),fixture.sessions[0].note);
  check(writes,[]);
  await evaluate('byId("editor").close();editing=null;render()');
  check(await evaluate(`${card("readonly")}.querySelector('[data-action="adjust"]').disabled`),true);
  check(await evaluate(`${card("readonly")}.draggable`),false);
  await evaluate(`${card("long")}.querySelector('[data-action="open"]').click()`);
  await wait('!opening');
  check(writes.map(write=>[write.path,write.body.id]),[["/api/open","long"]]);
  await evaluate(`${card("long")}.querySelector('[data-action="adjust"]').focus()`);
  fixture.sessions = fixture.sessions.filter(row=>row.id !== "long");
  await refresh();
  check(await evaluate('document.activeElement.classList.contains("column-sort")'),true);
  check(await evaluate('document.activeElement.closest(".column").dataset.state'),"review");
  fixture.sessions.find(row=>row.id === "short").readOnly = true;
  await refresh();
  check(await evaluate(`${card("short")}.querySelector('[data-action="adjust"]').disabled`),true);
  for (const theme of ["dark","light"]) {
    await page.command("Emulation.setEmulatedMedia",{features:[{name:"prefers-color-scheme",value:theme}]});
    check(await evaluate('getComputedStyle(document.body).backgroundColor'),theme === "dark" ? "rgb(28, 29, 32)" : "rgb(255, 255, 255)");
  }
  fixture.sessions.forEach(row=>{row.readOnly=false;});
  await refresh();
  const projectBaseline = structuredClone(fixture.projectCatalog), healthBaseline = structuredClone(fixture.health);
  check(await evaluate('hostHealth({value:"local",hostLabel:"本机"}).ready'),true);
  for (const patch of [{loaded:false},{pending:true},{cached:true},{incomplete:true},{error:"项目读取失败"}]) {
    fixture.projectCatalog = {...projectBaseline,...patch};
    await refresh();
    check(await evaluate('Boolean(hostHealth({value:"local",hostLabel:"本机"}).ready)'),false);
    check(await evaluate('byId("health-summary").textContent.includes("连接正常")'),false);
  }
  for (const patch of [{runtimeVerified:false},{nativeVerified:false},{scanComplete:false},{lastSuccessAt:null}]) {
    fixture.projectCatalog = structuredClone(projectBaseline);fixture.health = {...healthBaseline,...patch};
    await refresh();
    check(await evaluate('Boolean(hostHealth({value:"local",hostLabel:"本机"}).ready)'),false);
    check(await evaluate('byId("health-summary").textContent.includes("连接正常")'),false);
  }
  fixture.health = structuredClone(healthBaseline);
  await refresh();
  refreshPerformance = await evaluate(`(() => {
    data.sessions=Array.from({length:1500},(_,index)=>({...data.sessions[0],id:"performance-"+index,note:"",state:"review"}));
    const initial=performance.now();render();const fullRenderMs=performance.now()-initial;
    const original=[...document.querySelectorAll(".card")],unchangedRenderMs=[];
    for(let cycle=0;cycle<8;cycle++){
      data=structuredClone(data);const start=performance.now();render();unchangedRenderMs.push(performance.now()-start);
    }
    return {rows:original.length,retained:original.filter(card=>card.isConnected).length,fullRenderMs,unchangedRenderMs};
  })()`);
  check(refreshPerformance.retained,1500);
  check(requests.filter(request=>request.method === "POST").length,1);
  baselineCases = cases;
  fixture.fourColumns = true;
  delete fixture.states.unknown;
  fixture.sessions.push({
    ...fixture.sessions[0],id:"unplaced",title:"未归列示例",state:null,note:"保留备注",
    placementReason:"Codex 原生位置为默认区",sectionVersion:"unsectioned",version:7,inputVersion:9,
    readOnly:false
  },{
    ...fixture.sessions[0],id:"offline",title:"离线示例",remote:true,hostId:"remote-air",
    hostLabel:"离线主机",state:null,placementReason:"上次确认位于自定义分区",
    canOpen:false,presence:"cachedUnknown",catalogCurrent:true
  });
  await refresh();
  check(await evaluate('[...byId("board").children].map(column=>column.dataset.state)'),
    ["in_progress","review","blocked","to_archive"]);
  check(await evaluate('byId("jump").children.length'),4);
  check(await evaluate('byId("unplaced").open'),false);
  check(await evaluate('byId("unplaced-summary").textContent'),"未归列会话 1");
  await evaluate(`byId("unplaced-summary").click();byId("filter-toggle").click();
    byId("search").value="未归列示例";byId("search").dispatchEvent(new Event("input"));`);
  check(await evaluate('byId("unplaced-cards").children.length'),1);
  check(await evaluate(`${card("unplaced")}.querySelector(".placement-reason").textContent`),"Codex 原生位置为默认区");
  await evaluate(`${card("unplaced")}.querySelector('[data-action="adjust"]').click()`);
  check(await evaluate('byId("editor").open'),true);
  check(await evaluate('[...byId("state").options].filter(option=>!option.disabled).map(option=>option.value)'),
    ["in_progress","review","blocked","to_archive"]);
  check(await evaluate('byId("state").value'),"");
  await evaluate('byId("edit-form").requestSubmit()');
  check(writes.length,1);
  failedPlacement = true;
  await evaluate('byId("state").value="review";byId("edit-form").requestSubmit()');
  await wait('!saving && byId("notice").textContent.includes("原生分区写入未确认")');
  check(await evaluate('byId("editor").open'),true);
  check(await evaluate('byId("note").value'),"保留备注");
  failedPlacement = false;
  await evaluate('byId("edit-form").requestSubmit()');
  await wait('!saving && !byId("editor").open && byId("unplaced").hidden');
  check(await evaluate(`${card("unplaced")}.closest(".column").dataset.state`),"review");
  check(writes.slice(1),Array.from({length:2},()=>({
    path:"/api/state",body:{id:"unplaced",state:"review",note:"保留备注",version:7,inputVersion:9,sectionVersion:"unsectioned"}
  })));
  await evaluate(`byId("search").value="离线示例";byId("host-filter").value="all";
    byId("host-filter").dispatchEvent(new Event("change"));`);
  check(await evaluate('byId("unplaced-summary").textContent'),"未归列会话 1");
  check(await evaluate(`${card("offline")}.classList.contains("offline")`),true);
  check(await evaluate(`${card("offline")}.draggable`),false);
  check(await evaluate(`${card("offline")}.querySelector('[data-action="adjust"]').disabled`),true);
  check(await evaluate(`${card("offline")}.querySelector('[data-action="open"]').disabled`),true);
  await evaluate(`${card("offline")}.click();openSession("offline")`);
  check(writes.length,3);
} finally {
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({cases:baselineCases,fourColumnCases:cases-baselineCases,writes,refreshPerformance,cleanup}));
