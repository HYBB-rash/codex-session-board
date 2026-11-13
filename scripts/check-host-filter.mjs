import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";

const html = await fs.readFile(new URL("../index.html",import.meta.url),"utf8");
const fixture = {
  states:{unknown:"待判断",review:"待审阅",to_archive:"待归档"},health:{
    remoteRegistry:[{hostId:"a",hostLabel:"同名主机",enabled:true},{hostId:"b",hostLabel:"同名主机",enabled:false},{hostId:"empty",hostLabel:"空主机",enabled:false}],
  },projectCatalog:{loaded:true,remoteHosts:[],items:[
    {id:"local",label:"项目",paths:["/local"]},
    {id:"a1",label:"项目 · 同名主机",paths:["/a1"],remote:true,hostId:"a",hostLabel:"同名主机"},
    {id:"a2",label:"项目 · 同名主机",paths:["/a2"],remote:true,hostId:"a",hostLabel:"同名主机"},
    {id:"b1",label:"项目 · 同名主机",paths:["/b"],remote:true,hostId:"b",hostLabel:"同名主机",cached:true},
  ]},sessions:[
    ["local","local",null],["a1","a1","a"],["a-default",null,"a"],["a-issue",null,"a"],["b1","b1","b"],
  ].map(([id,projectId,hostId]) => ({id,projectId,hostId,remote:Boolean(hostId),hostLabel:"同名主机",
    title:"共同关键词 "+id,cwd:"/example",state:"to_archive",note:"",version:1,inputVersion:2,
    sectionVersion:"fixture",readOnly:hostId === "b",canArchive:hostId !== "b",
    projectIssue:id === "a-issue" ? "worktree unavailable" : null})),
};
const writes = [];
let pendingNew = false, releaseNew;
const server = http.createServer(async (request,response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST") {
    writes.push({path:request.url,body:JSON.parse(body)});
    if (request.url === "/api/new" && pendingNew) await new Promise(resolve => {releaseNew = resolve;});
    response.writeHead(200,{"Content-Type":"application/json"});
    response.end(JSON.stringify({ok:true}));
  } else {
    response.writeHead(200,{"Content-Type":request.url === "/" ? "text/html; charset=utf-8" : "application/json"});
    response.end(request.url === "/" ? html : JSON.stringify(fixture));
  }
});
await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
let browser, cases = 0;
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
    assert.fail("页面状态未就绪："+expression);
  };
  const select = (id,value) => evaluate(`byId(${JSON.stringify(id)}).value=${JSON.stringify(value)};byId(${JSON.stringify(id)}).dispatchEvent(new Event("change"))`);
  const ids = () => evaluate("visibleSessions().map(row=>row.id)");
  await wait("data.sessions.length === 5");
  check(await evaluate('byId("project-filter").disabled'),true);
  check(await evaluate('byId("host-filter").options.length'),5);
  check(await evaluate('Array.from(byId("host-filter").options).find(option=>option.value==="host:a").textContent'),"同名主机 · a");
  await select("host-filter","host:a");
  check(await ids(),["a1","a-default","a-issue"]);
  check(await evaluate('Array.from(byId("project-filter").options).map(option=>option.textContent)'),["全部项目","默认工作区","项目 · /a1","项目 · /a2"]);
  await select("project-filter","default");
  check(await ids(),["a-default"]);
  await select("project-filter","project:a1");
  await evaluate('byId("filter-toggle").click();byId("search").value="共同关键词";byId("search").dispatchEvent(new Event("input"))');
  check(await ids(),["a1","a-default","a-issue"]);
  await select("host-filter","host:b");
  check(await evaluate("selectedProject"),"all");
  check(await ids(),["b1"]);
  check(await evaluate('document.querySelector("[data-archive-all]").disabled'),true);
  check(writes,[]);
  await select("host-filter","host:empty");
  check(await ids(),[]);
  await evaluate('byId("new-session").click()');
  check(await evaluate('byId("new-target-open").disabled'),true);
  check(await evaluate('byId("new-host").value'),"host:empty");
  await evaluate('byId("new-target-cancel").click()');
  await select("host-filter","host:a");
  await evaluate('byId("new-session").click()');
  check(await evaluate('Array.from(byId("new-project").options).map(option=>option.value)'),["a1","a2"]);
  await select("host-filter","local");
  check(await evaluate("selectedHost"),"host:a");
  pendingNew = true;
  await evaluate('byId("new-target-form").requestSubmit()');
  await wait("creating");
  for (let attempt = 0; attempt < 100 && !releaseNew; attempt++) await new Promise(resolve => setTimeout(resolve,25));
  assert(releaseNew,"远程新建请求未送达");
  check(writes.at(-1).body.projectId,"a1");
  check(typeof writes.at(-1).body.requestId,"string");
  await select("host-filter","host:b");
  check(await evaluate("selectedHost"),"host:a");
  pendingNew = false;releaseNew();
  await wait("!creating && !byId('host-filter').disabled");
  await evaluate('byId("filter-toggle").click()');
  await select("project-filter","project:a1");
  await evaluate('archiveSessions(visibleSessions(),document.querySelector("[data-archive-all]"))');
  check(writes.filter(row=>row.path === "/api/archive").map(row=>row.body),[{id:"a1",version:1,inputVersion:2,sectionVersion:"fixture"}]);
  await evaluate('data.projectCatalog.items=data.projectCatalog.items.filter(item=>item.id!=="a1");data.health.remoteError="temporary";projectOptionsKey="";render()');
  check(await evaluate("selectedProject"),"project:a1");
  await evaluate('data.health.remoteError=null;render()');
  check(await evaluate("[selectedHost,selectedProject]"),["host:a","all"]);
  await evaluate('data.sessions=[];data.projectCatalog.items=[];data.health.remoteRegistry=[];render();render()');
  check(await evaluate("[selectedHost,byId('host-filter').selectedOptions[0].textContent.match(/已移除/g).length]"),["host:a",1]);
  await select("host-filter","all");
  await evaluate('byId("new-session").click()');
  check(await evaluate('byId("new-host").disabled'),false);
  check(await evaluate('byId("new-project").value'),"");
  await evaluate('byId("new-target-cancel").click()');
  console.log(JSON.stringify({cases}));
} finally {
  releaseNew?.();
  if (browser) await browser.close();
  await new Promise(resolve => server.close(resolve));
}
