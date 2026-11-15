import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";

const html = await fs.readFile(new URL("../index.html",import.meta.url),"utf8");
const project = (id,hostId,origin) => ({id,hostId,origin,remote:true,hostLabel:hostId,
  label:"同名项目",paths:["/target/" + id],canCreate:origin === "clientRemote"});
const fixture = {
  states:{unknown:"待判断",review:"待审阅",to_archive:"待归档"},health:{
    remoteRegistry:[{hostId:"a",hostLabel:"a",enabled:true},{hostId:"b",hostLabel:"b",enabled:false}],
    remoteConnections:[
      {hostId:"a",hostLabel:"a",connected:true,runtimeVerified:true,lastSuccessAt:1,scanComplete:true,
        catalog:{complete:true,cached:false,capturedAt:1700000000,coverage:{active:true,archived:true},issues:[],pending:false}},
      {hostId:"b",hostLabel:"b",connected:false,enabled:false,
        catalog:{complete:false,cached:true,capturedAt:1600000000,coverage:{active:true,archived:false},issues:["b-only-issue"],error:"b-only-error"}},
    ],
  },projectCatalog:{loaded:true,items:[
    {id:"local",label:"本地",paths:["/local"]},project("server","a","targetServer"),
    project("empty","a","targetLegacy"),project("client","a","clientRemote"),project("other","b","targetServer"),
  ]},sessions:[
    {id:"local",projectId:"local",remote:false},
    {id:"active",hostId:"a",projectId:"server",presence:"active",catalogCurrent:true,archived:false},
    {id:"archived",hostId:"a",projectId:"server",presence:"archived",catalogCurrent:true,archived:true},
    {id:"old",hostId:"a",projectId:"server",presence:"cachedUnknown",catalogCurrent:false,archived:false},
    {id:"last-active",hostId:"a",projectId:"server",presence:"cachedUnknown",catalogCurrent:true,archived:false},
    {id:"last-archive",hostId:"a",projectId:"server",presence:"cachedUnknown",catalogCurrent:true,archived:true},
    {id:"legacy",hostId:"a",projectId:"client"},
    {id:"other",hostId:"b",projectId:"other",presence:"cachedUnknown",catalogCurrent:true,archived:false},
  ].map(row => ({remote:true,title:"关键词 "+row.id,cwd:"/target",hostLabel:row.hostId,state:"to_archive",
    note:"",version:1,inputVersion:1,canArchive:true,...row})),
};
const writes = [];
const server = http.createServer(async (request,response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST") writes.push({path:request.url,body:JSON.parse(body)});
  response.writeHead(200,{"Content-Type":request.url === "/" ? "text/html; charset=utf-8" : "application/json"});
  response.end(request.url === "/" ? html : JSON.stringify(request.method === "POST" ? {ok:true} : fixture));
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
  await wait('typeof data !== "undefined" && data.sessions.length === 8');
  check(await ids(),["local"]);
  check(await evaluate('byId("history-filter").disabled'),true);
  await select("host-filter","host:a");
  await wait('!byId("host-filter").disabled');
  check(writes,[]);
  check(await ids(),["active","last-active","legacy"]);
  check(await evaluate('byId("count").textContent'),"显示 3 / 当前主机范围共 3 条");
  check(await evaluate('Array.from(byId("project-filter").options).map(option=>option.value)'),
    ["all","default","project:server","project:empty","project:client"]);
  check(await evaluate('projectLabel(data.projectCatalog.items.find(item=>item.id==="server")).includes("主机项目")'),true);
  check(await evaluate('projectLabel(data.projectCatalog.items.find(item=>item.id==="client")).includes("本机登记")'),true);
  check(await evaluate('(()=>{const catalog=data.health.remoteConnections[0].catalog;catalog.cached=true;catalog.complete=false;const status=hostHealth({value:"host:a",hostId:"a",hostLabel:"a",enabled:true}).status;catalog.cached=false;catalog.complete=true;return status.includes("显示上次主机目录快照")})()'),true);
  check(await evaluate('byId("filter-scope").textContent.includes("活动")'),true);
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).diagnostics.some(([,message])=>message.includes("b-only"))'),false);
  await evaluate('data.projectCatalog.loaded=false;render()');
  check(await evaluate('byId("project-filter").disabled'),false);
  await evaluate('data.projectCatalog.loaded=true;data.health.remoteConnections[0].catalog.pending=true;render()');
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).status'),"正在更新主机目录");
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).ready'),false);
  await evaluate('data.health.remoteConnections[0].catalog.pending=false;data.health.remoteConnections[0].catalog.complete=false;render()');
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).status'),"主机目录尚未读取完整");
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).ready'),false);
  await evaluate('data.health.remoteConnections[0].catalog.complete=true;render()');
  await evaluate('data.sessions=data.sessions.map(row=>row.archived || row.presence==="cachedUnknown" ? {...row,readOnly:true} : row);render()');
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).status'),"主机目录已读取完整");
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).ready'),true);
  await evaluate('data.health.remoteConnections[0].catalog.cached=true;render()');
  check(await evaluate('hostHealth({value:"host:a",hostId:"a",hostLabel:"a"}).ready'),false);
  await evaluate('data.health.remoteConnections[0].catalog.cached=false;render()');
  await select("history-filter","archived");
  check(await ids(),["archived","last-archive"]);
  check(await evaluate('byId("count").textContent'),"显示 2 / 当前主机范围共 2 条");
  check(await evaluate('document.querySelector("[data-archive-all]").disabled'),true);
  await select("history-filter","all");
  check(await ids(),["active","archived","last-active","last-archive","legacy"]);
  await select("history-filter","cached");
  check(await ids(),["old","last-active","last-archive"]);
  check(await evaluate('document.querySelector("[data-archive-all]").disabled'),true);
  check(await evaluate('document.querySelector("[data-id=old]").textContent.includes("历史缓存")'),true);
  check(await evaluate('document.querySelector("[data-id=old]").draggable'),false);
  await select("project-filter","project:empty");
  check(await ids(),[]);
  await evaluate('byId("new-session").click()');
  check(writes.filter(row=>row.path === "/api/new"),[]);
  check(await evaluate('byId("feedback").textContent.includes("仅供查看")'),true);
  await evaluate('createSession("server")');
  check(writes.filter(row=>row.path === "/api/new"),[]);
  await select("project-filter","all");
  await evaluate('byId("new-session").click()');
  check(await evaluate('Array.from(byId("new-project").options).map(option=>option.value)'),["client"]);
  check(await evaluate('byId("history-filter").disabled'),true);
  await evaluate('byId("new-target-cancel").click()');
  await select("history-filter","active");
  await select("project-filter","project:client");
  await evaluate('byId("new-session").click()');
  await wait("!creating");
  check(writes.filter(row=>row.path === "/api/new").map(row=>row.body.projectId),["client"]);
  check(writes.filter(row=>row.path === "/api/new").map(row=>row.body),[{projectId:"client"}]);
  check(await evaluate('byId("feedback").textContent.includes("远程项目新建页面打开请求")'),true);
  await select("host-filter","host:b");
  check(await ids(),["other"]);
  check(writes.filter(row=>row.path === "/api/connect"),[]);
  check(writes.filter(row=>row.path === "/api/refresh").length,0);
  check(await evaluate('hostHealth({value:"host:b",hostId:"b",hostLabel:"b",enabled:false}).status.includes("缓存时间")'),true);
  check(await evaluate('hostHealth({value:"host:b",hostId:"b",hostLabel:"b",enabled:false}).diagnostics.some(([,message])=>message.includes("b-only-error"))'),true);
  await evaluate('data.health.remoteConnections[1].catalog.error="b-only-error";selectedProject="project:missing";projectOptionsKey="";render()');
  check(await evaluate("selectedProject"),"project:missing");
  await select("host-filter","host:a");
  await wait("selectedHost === 'host:a'");
  await evaluate('selectedProject="project:missing";projectOptionsKey="";render()');
  check(await evaluate("selectedProject"),"all");
  await select("history-filter","all");
  await evaluate('archiveSessions(visibleSessions(),document.querySelector("[data-archive-all]"))');
  check(writes.filter(row=>row.path === "/api/archive").map(row=>row.body.id),["active","legacy"]);
  await select("history-filter","archived");
  await evaluate('byId("filter-toggle").click();byId("search").value="关键词";byId("search").dispatchEvent(new Event("input"))');
  check(await ids(),["archived","last-archive"]);
  await select("host-filter","local");
  check(await evaluate("[selectedHistory,byId('history-filter').disabled]"),["active",true]);
  check(await ids(),["local"]);
  check(writes.some(row=>row.path === "/api/connect"),false);
  await evaluate('data.health.remoteConnections[0].connected=false;data.health.remoteConnections[0].runtimeVerified=false;render()');
  const beforeOffline = writes.length;
  await select("host-filter","host:a");
  check(writes.length,beforeOffline);
  const cleanup = await browser.close();browser = null;
  console.log(JSON.stringify({ok:true,cases,isolatedBrowser:true,cleanup}));
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
