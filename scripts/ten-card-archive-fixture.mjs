import assert from "node:assert/strict";
import http from "node:http";
import { once } from "node:events";

const mixedOutcomes = ["archived","archived","archived","archived","newInput","rejected","pending"];

export async function tenCardArchiveFixture(html, {outcomes = mixedOutcomes, remote = false} = {}) {
  const cards = Array.from({length:10}, (_, index) => ({
    id:`11111111-1111-4111-8111-${String(index + 1).padStart(12, "0")}`,
    title:`归档样本 ${String(index + 1).padStart(2, "0")}`, state:"to_archive", note:"",
    version:index + 1, inputVersion:20 + index, sectionVersion:`section-${index}`,
    updatedAt:100 - index, cwd:"/fixture", readOnly:false, canArchive:true, canOpen:true,
  }));
  if (remote) {
    const subject = cards[3];
    for (const [index,card] of cards.entries()) {
      const hostId = index === 0 ? "other" : "mac";
      const threadId = index === 0 ? subject.id : card.id;
      if (index === 0) Object.assign(card,{version:subject.version,inputVersion:subject.inputVersion,sectionVersion:subject.sectionVersion});
      Object.assign(card,{id:`remote:${JSON.stringify(["codex",hostId,threadId])}`,threadId,hostId,hostLabel:hostId,
        remote:true,catalogCurrent:true,presence:"active",ordinary:true});
    }
  }
  const messages = {newInput:"有新输入，本次未提交原生归档", rejected:"当前状态不允许归档"};
  const ledger = [], errors = [];
  const blocked = Promise.withResolvers();
  const oldRead = Promise.withResolvers();
  const unavailable = Symbol("snapshot unavailable");
  const nextBatchResponse = Promise.withResolvers();
  const cancelled = Symbol("cancelled");
  let batch, nextTargets, delayedHistory, prematureBatch, canVerify = false;
  let nextRefresh, snapshot, restoredSessions, holdSnapshot = false, failReads = false;
  const snapshots = [];

  function handle(method, url, body, requestNumber) {
    if (method === "GET" && url === "/api/sessions") {
      if (failReads) return unavailable;
      const sessions = snapshot ?? restoredSessions ?? cards.filter(card => !["archived","verified"].includes(status(card.id)))
        .map(card => status(card.id) === "pending" ? {...card,archivePending:true} : card);
      if (snapshot) {
        snapshots.push({request:requestNumber,sessions:structuredClone(sessions)});
        snapshot = undefined;
      }
      const result = {states:{review:"待审阅",to_archive:"待归档"}, health:{},
        projectCatalog:{loaded:true,items:[],remoteHosts:[]}, sessions};
      if (holdSnapshot) {
        holdSnapshot = false;
        return oldRead.promise.then(() => result);
      }
      return result;
    }
    if (method === "GET" && url === "/api/archive-batches") {
      const snapshot = {batches:batch ? [structuredClone(batch)] : []};
      return delayedHistory ? delayedHistory.promise.then(() => snapshot) : snapshot;
    }
    if (method === "GET" && url === "/api/plugin-update") return {};
    assert.equal(method, "POST", `未识别的请求 ${method} ${url}`);
    if (url === "/api/refresh") {
      assert.deepEqual(body,{});
      assert(nextRefresh,"未安排的刷新");
      snapshot = nextRefresh.sessions;
      holdSnapshot = nextRefresh.hold;
      failReads = false;
      if (nextRefresh.persist) restoredSessions = snapshot;
      nextRefresh = undefined;
      return {ok:true};
    }
    if (url === "/api/archive-batches") {
      assert.match(body.id, /^[0-9a-f-]{36}$/);
      const expected = cards.map(({id,title,version,inputVersion,sectionVersion}) =>
        ({id,title,version,inputVersion,sectionVersion}));
      if (delayedHistory) {
        prematureBatch = {id:body.id,createdAt:1760000001,finishedAt:null,
          items:body.items.map(item => ({...item,status:"awaiting",message:null}))};
        return {batch:prematureBatch};
      }
      if (nextTargets) {
        assert.notEqual(body.id,batch.id, "另行归档不得复用原批次");
        const byId = (first,second) => first.id.localeCompare(second.id);
        assert.deepEqual([...body.items].sort(byId),expected.filter(card => nextTargets.includes(card.id)).sort(byId));
        nextTargets = undefined;
        return nextBatchResponse.promise;
      }
      assert.equal(batch, undefined, "意外创建第二个批次");
      const byId = (first, second) => first.id.localeCompare(second.id);
      assert.deepEqual([...body.items].sort(byId), expected.sort(byId));
      batch = {id:body.id, createdAt:1760000000, finishedAt:null,
        items:body.items.map(item => ({...item,status:"awaiting",message:null}))};
      return {batch};
    }
    if (url === "/api/open") {
      assert.equal(status(body.id), "newInput");
      return {ok:true};
    }
    if (url === "/api/state") {
      const card = cards.find(card => card.id === body.id);
      assert.equal(card?.followUp,true);
      const {id,state,note,version,inputVersion,sectionVersion} = card;
      assert.deepEqual(body,{id,state,note,version,inputVersion,sectionVersion});
      card.followUp = false;
      card.version++;
      return {ok:true,sectionVersion,session:{...card}};
    }
    if (url === "/api/archive-batches/check") {
      assert.deepEqual(body, {id:batch.id});
      if (canVerify) for (const item of batch.items) {
        if (item.status === "pending") {
          item.status = "verified";
          failReads = true;
        }
      }
      return {batch};
    }
    assert.equal(url, "/api/archive", `意外请求 ${url}`);
    if (prematureBatch && body.batchId === prematureBatch.id) {
      const target = prematureBatch.items.find(item => item.id === body.id);
      assert(target);
      if (status(target.id) === "pending") return nextBatchResponse.promise.then(() => ({item:target}));
      return {item:{...target,status:"archived"}};
    }
    const index = batch.items.findIndex(item => item.id === body.id);
    assert(index >= 0);
    const item = batch.items[index];
    const {id,version,inputVersion,sectionVersion} = item;
    assert.deepEqual(body, {id,version,inputVersion,sectionVersion,batchId:batch.id});
    assert.equal(item.status, "awaiting", `同一批次重复归档 ${id}`);
    assert(index < outcomes.length, `请求中断后仍在提交 ${id}`);
    item.status = outcomes[index];
    item.message = messages[item.status] ?? null;
    if (item.status === "newInput") {
      const card = cards.find(card => card.id === item.id);
      card.inputVersion++;
      card.followUp = true;
    }
    return {item};
  }
  function status(id) { return batch?.items.find(item => item.id === id)?.status; }
  const server = http.createServer(async (request, response) => {
    try {
      const {method, url} = request;
      if (method === "GET" && url === "/") {
        response.writeHead(200, {"Content-Type":"text/html; charset=utf-8"}).end(html);
        return;
      }
      if (method === "GET" && url === "/test-click-observed") { response.writeHead(204).end(); return; }
      if (!url.startsWith("/api/")) { response.writeHead(404).end(); return; }
      let text = "";
      for await (const chunk of request) text += chunk;
      const body = text ? JSON.parse(text) : null;
      const requestNumber = ledger.push({method,url,body});
      const reply = await handle(method, url, body, requestNumber);
      if (url === "/api/sessions") ledger[requestNumber - 1].responseStatus = reply === unavailable ? 503 : 200;
      if (reply === unavailable) {
        response.writeHead(503,{"Content-Type":"application/json","Cache-Control":"no-store"}).end(JSON.stringify({error:"读取根快照失败"}));
        return;
      }
      if (reply === cancelled) {
        response.writeHead(409,{"Content-Type":"application/json"}).end(JSON.stringify({error:"测试已结束本次建批探测"}));
        return;
      }
      if (url === "/api/archive" && reply.item.status === "pending") await blocked.promise;
      response.writeHead(200, {"Content-Type":"application/json","Cache-Control":"no-store"})
        .end(JSON.stringify(reply));
    } catch (error) {
      errors.push(`${request.method} ${request.url}: ${error.message}`);
      response.writeHead(500, {"Content-Type":"application/json"}).end(JSON.stringify({error:error.message}));
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return {url:`http://127.0.0.1:${server.address().port}/`, ledger, errors, snapshots,
    batch:() => structuredClone(batch), allowVerification:() => { canVerify = true; },
    releaseOldRead() { oldRead.resolve(); },
    prepareRefresh(label, changed, persist, hold = false) {
      const visible = cards.filter(card => !["archived","verified"].includes(status(card.id)));
      const targets = changed.map(({id,...fields}) => {
        const card = cards.find(row => row.id === id);
        assert(card && !visible.includes(card));
        return {...card,...fields};
      });
      const sessions = [...structuredClone(visible),...targets];
      const marker = sessions.find(card => status(card.id) === "rejected");
      marker.note = label;
      nextRefresh = {sessions,persist,hold};
      return structuredClone(sessions);
    },
    holdNextBatchFor(ids) { nextTargets = ids; },
    cancelNextBatch() { nextBatchResponse.resolve(cancelled); },
    holdHistory() { delayedHistory = Promise.withResolvers(); },
    releaseHistory() { delayedHistory?.resolve(); delayedHistory = undefined; },
    release:() => blocked.resolve(), async close() {
      blocked.resolve();
      oldRead.resolve();
      delayedHistory?.resolve();
      nextBatchResponse.resolve({});
      const closed = once(server, "close");
      server.close();
      server.closeAllConnections();
      await closed;
    }};
}
