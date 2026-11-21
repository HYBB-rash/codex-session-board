import assert from "node:assert/strict";
import http from "node:http";
import { clickNode, inputText, until } from "./browser-actions.mjs";

export async function checkPackagedEditor(page, readSnapshot) {
  const submitted = ["  编辑器实测  ",""];
  const card = (await readSnapshot()).sessions.find(row => !row.remote && !row.readOnly && row.state);
  assert(card);
  const selector = `.card[data-id=${JSON.stringify(card.id)}] [data-action=adjust]`;
  const receipts = [];
  for (const note of submitted) {
    await until(() => page.evaluate(`Boolean(document.querySelector(${JSON.stringify(selector)}))`),"包内卡片未出现");
    await clickNode(page,selector);
    await until(() => page.evaluate('document.activeElement.id==="note"'),"包内编辑器未取得焦点");
    await inputText(page,"#note",note);
    await clickNode(page,"#save");
    await until(() => page.evaluate('!document.getElementById("editor").open && document.activeElement.dataset.action==="adjust"'),
      "包内保存未完成");
    const row = (await readSnapshot()).sessions.find(row => row.id === card.id);
    assert.equal(row.note,note.trim());
    assert.equal(row.state,card.state);
    assert.deepEqual(await page.evaluate(`data.sessions.find(row=>row.id===${JSON.stringify(card.id)})`),row);
    receipts.push({id:row.id,state:row.state,note:row.note,version:row.version,inputVersion:row.inputVersion});
  }
  return {submitted,receipts};
}

export async function checkEditorDrafts(browser, html) {
  const local = {id:"same",threadId:"same",title:"本机卡片",state:"review",note:"原备注",version:4,
    inputVersion:7,sectionVersion:"first",projectId:"",updatedAt:1,source:"codex"};
  const remote = {...local,id:'remote:["codex","mac","same"]',hostId:"mac",hostLabel:"Mac",remote:true,
    title:"远程卡片",note:"远程原备注"};
  const data = {states:{review:"待审阅",blocked:"已阻塞",to_archive:"待归档"},sessions:[local,remote],
    health:{scanComplete:true,runtimeVerified:true,nativeVerified:true},projectCatalog:{loaded:true,items:[]}};
  const posts = [];
  let mode = "conflict";
  const server = http.createServer(async (request,response) => {
    if (request.url === "/") return response.writeHead(200,{"content-type":"text/html;charset=utf-8"}).end(html);
    const send = (status,result) => response.writeHead(status,{"content-type":"application/json"}).end(JSON.stringify(result));
    if (request.url === "/api/sessions") return send(200,data);
    if (request.url !== "/api/state") return send(200,{batches:[]});
    let text = "";
    for await (const chunk of request) text += chunk;
    const input = JSON.parse(text);
    posts.push(input);
    if (mode === "stale") return send(200,{ok:true,session:{...local,version:input.version-1,note:"旧回执"}});
    if (mode === "another") return send(200,{ok:true,session:{...remote,id:local.id,note:"另一主机回执"}});
    if (mode === "conflict") {
      Object.assign(local,{state:"to_archive",version:5,inputVersion:8,sectionVersion:"new-input",followUp:true});
      return send(409,{error:"有新的输入，请重新核对"});
    }
    Object.assign(local,{state:"blocked",note:input.note.trim(),version:6,sectionVersion:"saved",followUp:false});
    return send(200,{ok:true,session:local});
  });
  await new Promise(resolve => server.listen(0,"127.0.0.1",resolve));
  try {
    const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
    const evaluate = expression => page.evaluate(expression);
    const wait = (expression, description) => until(() => evaluate(expression), description);
    const field = id => evaluate(`document.getElementById("${id}").value`);
    const open = async card => {
      const button = `[...document.querySelectorAll(".card")].find(row=>row.dataset.id===${JSON.stringify(card.id)})?.querySelector("[data-action=adjust]")`;
      await wait(`Boolean(${button})`,"未显示 "+card.title);
      await evaluate(`${button}.click()`);
      await wait('document.activeElement?.id === "note"',card.title+"的备注未获得焦点");
    };
    const close = async () => {
      await evaluate('document.getElementById("cancel").click()');
      await wait('!document.getElementById("editor").open && document.activeElement.dataset.action==="adjust"',"焦点未回到调整按钮");
    };
    const select = (id,value) => evaluate(`(()=>{const node=document.getElementById("${id}");node.value=${JSON.stringify(value)};node.dispatchEvent(new Event("change"))})()`);
    const reload = async () => {
      const loaded = page.event("Page.loadEventFired");
      await page.command("Page.reload",{ignoreCache:true});await loaded;
    };
    const save = () => evaluate('document.getElementById("save").click()');
    const notice = message => wait(`document.getElementById("notice").textContent.includes(${JSON.stringify(message)})`,message);

    await open(local);await inputText(page,"#note","  本机草稿  ");await reload();
    await open(local);assert.equal(await field("note"),"  本机草稿  ");await notice("已恢复");await close();
    await select("host-filter","host:mac");await open(remote);assert.equal(await field("note"),"远程原备注");
    await inputText(page,"#note","");await reload();
    await open(local);assert.equal(await field("note"),"  本机草稿  ");await close();
    await select("host-filter","host:mac");await open(remote);assert.equal(await field("note"),"");await notice("已恢复");await close();
    assert.deepEqual(posts,[]);

    await select("host-filter","local");await open(local);await save();await notice("新的输入");
    await evaluate('[...document.querySelectorAll("#edit-form button")].find(button=>button.textContent==="重新核对").click()');
    await notice("已读取当前状态");
    assert.equal(await field("state"),"to_archive");assert.equal(await field("note"),"  本机草稿  ");
    assert.equal(posts.length,1);
    for (const [fault,message] of [["stale","旧保存回执"],["another","缺少有效卡片"]]) {
      mode = fault;await save();await notice(message);
      assert.equal(await field("note"),"  本机草稿  ");
      assert.equal(await evaluate('document.getElementById("editor").open'),true);
    }
    mode = "success";await save();
    await wait('document.activeElement?.dataset.action === "adjust" && !document.getElementById("editor").open',"成功后焦点未回到卡片");
    assert.deepEqual(posts.map(item=>[item.id,item.version,item.inputVersion,item.sectionVersion]),
      [["same",4,7,"first"],...Array.from({length:3},()=>["same",5,8,"new-input"])]);
    await open(local);assert.equal(await field("state"),"blocked");assert.equal(await field("note"),"本机草稿");
    assert.equal(await evaluate('document.getElementById("notice").textContent'),"");await close();
    await select("host-filter","host:mac");await open(remote);assert.equal(await field("note"),"");await close();
    return {reload:true,hostIsolation:true,blank:true,conflictReadback:true,staleAndWrongHost:true,focus:true};
  } finally { await new Promise(resolve => server.close(resolve)); }
}
