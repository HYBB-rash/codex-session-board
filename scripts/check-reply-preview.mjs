import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import http from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { launchBrowser } from "./browser-client.mjs";

const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
const states = {in_progress:"进行中",review:"待审阅",blocked:"已阻塞",to_archive:"待归档"};
const session = (id, state, updatedAt) => ({
  id,state,updatedAt,title:`会话 ${id}`,version:0,inputVersion:0,sectionVersion:"fixture",
});
const subject = {...session("subject","review",1760000000),previewTurnId:"first"};
const fixtures = {states,health:{},fourColumns:true,projectCatalog:{loaded:true,items:[]},
  sessions:[subject,session("running","in_progress",1760000001),
    ...Array.from({length:15},(_,index)=>session(`review-${index}`,"review",1760000001+index))]};
const first = "第一轮短答复";
const second = "第二轮最终答复\n\n" + "<img src=x onerror=alert(1)>\n正文\n".repeat(150) + "最后一行";
const requests = [];
let intercept = false, pending;
const server = http.createServer(async (request, response) => {
  if (request.url === "/") return response.writeHead(200,{"Content-Type":"text/html; charset=utf-8"}).end(html);
  if (request.url === "/api/sessions") return response.writeHead(200,{"Content-Type":"application/json"}).end(JSON.stringify(fixtures));
  if (request.method !== "POST") return response.writeHead(404).end();
  let encoded = "";
  for await (const chunk of request) encoded += chunk;
  const body = JSON.parse(encoded || "{}");
  requests.push({path:request.url,body});
  response.setHeader("Content-Type","application/json");
  if (request.url === "/api/preview-reply") {
    if (intercept) {
      pending = response;
      return;
    }
    if (subject.previewTurnId !== body.turnId) return response.writeHead(400).end('{"error":"changed"}');
    return response.end(JSON.stringify({turnId:body.turnId,text:body.turnId === "first" ? first : second}));
  }
  if (request.url === "/api/open") return response.end('{"ok":true}');
  response.writeHead(404).end('{"error":"unknown route"}');
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
let browser, cleanup;
const card = '[data-id="subject"]';
try {
  browser = await launchBrowser();
  const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
  const evaluate = expression=>page.evaluate(expression);
  const wait = async predicate => {
    for (let attempt=0;attempt<100;attempt++) {
      if (await predicate()) return;
      await delay(25);
    }
    assert.fail("预览测试未收到预期结果");
  };
  const trigger = `document.querySelector('${card} [data-action="preview"]')`;
  const preview = () => evaluate(`${trigger}.click()`);
  const text = () => evaluate('byId("reply-text").textContent');
  await wait(()=>evaluate(`Boolean(${trigger})`));
  assert(!await evaluate('Boolean(document.querySelector(\'[data-id="running"] [data-action="preview"]\'))'));
  assert.deepEqual(requests,[]);

  const scroll = await evaluate(`(() => {
    const list = document.querySelector("#column-review .cards");
    list.scrollTop = list.scrollHeight;
    return list.scrollTop;
  })()`);
  assert(scroll > 0);
  await preview();
  await wait(async()=>await text()===first);
  assert.deepEqual(requests[0],{path:"/api/preview-reply",body:{id:"subject",turnId:"first"}});
  await page.command("Input.dispatchKeyEvent",{type:"keyDown",key:"Escape",code:"Escape",windowsVirtualKeyCode:27});
  await wait(()=>evaluate('!byId("reply-reader").open'));
  assert(await evaluate(`document.activeElement === ${trigger}`));

  delete subject.previewTurnId;
  await preview();
  await wait(()=>evaluate('!byId("reply-reader").open && !document.querySelector(\'[data-id="subject"] [data-action="preview"]\')'));
  assert.equal(await text(),"");
  subject.previewTurnId = "second";
  await evaluate("load()");
  await preview();
  await wait(async()=>await text()===second);
  assert(await evaluate('!byId("reply-text").querySelector("img") && byId("reply-body").scrollHeight > byId("reply-body").clientHeight'));
  await evaluate('byId("reply-body").scrollTop = byId("reply-body").scrollHeight; byId("reply-close").click()');
  assert.equal(await evaluate('document.querySelector("#column-review .cards").scrollTop'),scroll);
  assert(await evaluate(`document.activeElement === ${trigger}`));
  assert.equal(await text(),"");

  intercept = true;
  await preview();
  await wait(()=>Boolean(pending));
  delete subject.previewTurnId;
  await evaluate("load()");
  assert(await evaluate(`!byId("reply-reader").open && !${trigger}`));
  pending.end(JSON.stringify({turnId:"second",text:second}));
  pending = null;
  await delay(100);
  assert.equal(await text(),"");
  await evaluate(`document.querySelector('${card} .card-title').click()`);
  await wait(()=>requests.some(request=>request.path==="/api/open" && request.body.id==="subject"));
  assert(!await evaluate('byId("reply-reader").open'));
  const center = selector=>evaluate(`(() => {
    const bounds = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();
    return {x:bounds.left+bounds.width/2,y:bounds.top+bounds.height/2};
  })()`);
  const source = await center(`${card} .card-title`);
  const target = await center("#column-blocked .cards");
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
  assert(await evaluate('byId("editor").open && !byId("reply-reader").open'));
  assert.equal(requests.filter(request=>request.path==="/api/open").length,1);
  assert(!requests.some(request=>request.path==="/api/state"));
} finally {
  pending?.end();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve=>server.close(resolve));
}
console.log(JSON.stringify({passed:true,cleanup}));
