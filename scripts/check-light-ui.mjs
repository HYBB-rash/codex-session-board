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
const server = http.createServer(async (request,response) => {
  requests.push({method:request.method,path:request.url});
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST") {
    writes.push({path:request.url,body:JSON.parse(body)});
    return response.writeHead(200,{"Content-Type":"application/json"}).end('{"ok":true}');
  }
  response.writeHead(200,{"Content-Type":request.url === "/" ? "text/html; charset=utf-8" : "application/json"});
  response.end(request.url === "/" ? html : JSON.stringify(fixture));
});
await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
let browser, cases = 0, cleanup;
const check = (actual,expected) => {assert.deepEqual(actual,expected);cases++;};
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
  await wait('typeof data !== "undefined" && data.sessions.length === 4');
  for (const [width,height] of [[320,256],[390,320],[1280,800]]) {
    await page.command("Emulation.setDeviceMetricsOverride",{width,height,deviceScaleFactor:1,mobile:false});
    await evaluate("new Promise(requestAnimationFrame)");
    check(await evaluate('byId("board-viewport").clientHeight >= 280'),true);
    check(await evaluate('document.documentElement.scrollWidth <= innerWidth'),true);
    if (height < 400) check(await evaluate('document.documentElement.scrollHeight > innerHeight'),true);
  }
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
  await evaluate('data.sessions.find(row=>row.id==="long").title="更新后的会话";render()');
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
  await wait('!saving');
  check(writes.map(write=>[write.path,write.body.id]),[["/api/open","long"]]);
  await evaluate(`${card("long")}.querySelector('[data-action="adjust"]').focus();
    data.sessions=data.sessions.filter(row=>row.id!=="long");render()`);
  check(await evaluate('document.activeElement.classList.contains("column-sort")'),true);
  check(await evaluate('document.activeElement.closest(".column").dataset.state'),"review");
  await evaluate('data.sessions.find(row=>row.id==="short").readOnly=true;render()');
  check(await evaluate(`${card("short")}.querySelector('[data-action="adjust"]').disabled`),true);
  for (const theme of ["dark","light"]) {
    await page.command("Emulation.setEmulatedMedia",{features:[{name:"prefers-color-scheme",value:theme}]});
    check(await evaluate('getComputedStyle(document.body).backgroundColor'),theme === "dark" ? "rgb(28, 29, 32)" : "rgb(255, 255, 255)");
  }
  check(requests.filter(request=>request.method === "POST").length,1);
} finally {
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({cases,writes,cleanup}));
