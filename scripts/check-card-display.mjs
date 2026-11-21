import assert from "node:assert/strict";

export async function checkCardDisplay(evaluate, snapshot, wait, clickControl) {
  const card = 'document.querySelector(".card[data-id=local]")';
  const position = `(() => {
    const list=document.querySelector("#column-review .cards"),top=list.getBoundingClientRect().top;
    const anchor=[...list.querySelectorAll(".card")].find(node=>node.getBoundingClientRect().bottom>top);
    return {id:anchor?.dataset.id,offset:anchor ? anchor.getBoundingClientRect().top-top : 0};
  })()`;
  snapshot.sessions[0].note = "需要保留的完整备注".repeat(120);
  await evaluate("load()");
  await wait(`${card}?.querySelector(".note-details.expandable")`);
  await evaluate(`${card}.querySelector(".note-details summary").click();
    ${card}.querySelector(".project-tag").open=true;
    ${card}.querySelector("[data-action=adjust]").focus({preventScroll:true});
    window.displayPosition=${position};`);
  snapshot.sessions[0].title = "当前卡片标题已更新";
  await evaluate("load()");
  await wait(`${card}.querySelector(".card-title").textContent === "当前卡片标题已更新"`);
  snapshot.sessions[0].observationWarning = "当前执行证据待确认";
  await evaluate("load()");
  await wait(`${card}.textContent.includes("当前执行证据待确认")`);
  assert(await evaluate(`${card}.querySelector(".note-details").open &&
    ${card}.querySelector(".project-tag").open &&
    document.activeElement === ${card}.querySelector("[data-action=adjust]")`));
  assert(await evaluate(`${card}.querySelector("time").title.startsWith("会话更新时间：")`));
  assert(await evaluate(`(() => {
    const current=${position};
    return current.id === displayPosition.id && Math.abs(current.offset-displayPosition.offset)<1;
  })()`), "刷新应保留正在阅读的卡片和偏移");
  delete snapshot.sessions[0].observationWarning;
  await evaluate("load()");
  await wait(`!${card}.textContent.includes("当前执行证据待确认")`);
  assert(await evaluate(`[...document.querySelectorAll(".column")].every(column =>
    Number(column.querySelector(".column-count").textContent) === column.querySelectorAll(".card").length)`));
  snapshot.sessions[0].updatedAt -= 100;
  await evaluate("load()");
  await wait('document.querySelector("#column-review .cards .card:last-child").dataset.id === "local"');
  assert(await evaluate(`document.activeElement === ${card}.querySelector("[data-action=adjust]")`),
    "同列重排后应可继续操作刚才聚焦的卡片");
  snapshot.sessions[0].readOnly = true;
  await evaluate("load()");
  await wait(`${card}.querySelector("[data-action=adjust]").disabled`);
  assert(await evaluate('document.activeElement.matches("#column-review .column-sort")'),
    "卡片变为只读时应把焦点移到原列可用的控件");
  snapshot.sessions[0].readOnly = false;
  snapshot.sessions[0].updatedAt += 100;

  const row = snapshot.sessions.find(session => session.id === "empty");
  const blockedCount = await evaluate('Number(document.querySelector("#column-blocked .column-count").textContent)');
  Object.assign(row, {state:null, title:"空会话", placementReason:"原生侧栏默认区"});
  await evaluate("load()");
  await wait('byId("unplaced-summary").textContent === "未归列会话 1"');
  await clickControl("#unplaced-summary");
  const title = '#unplaced-cards [data-id="empty"] .card-title';
  assert.equal(await evaluate(`document.querySelector(${JSON.stringify(title)}).textContent`), row.title);
  assert.equal(await evaluate('document.querySelector("#unplaced-cards .placement-reason").textContent'), row.placementReason);
  assert.equal(await evaluate('Number(document.querySelector("#column-blocked .column-count").textContent)'), blockedCount - 1);
  await clickControl(title);
  await wait('!byId("reply-reader").hidden && byId("reply-status").textContent.includes("还没有完整回复")');
  await clickControl("#reply-close");
  await wait(`byId("reply-reader").hidden && document.activeElement.matches(${JSON.stringify(title)})`);
  const second = snapshot.sessions.find(session => session.id === "review-0");
  Object.assign(second, {state:null, updatedAt:row.updatedAt + 1, placementReason:"新项目未归列"});
  await evaluate("load()");
  await wait('byId("unplaced-summary").textContent === "未归列会话 2"');
  assert.deepEqual(await evaluate('Array.from(byId("unplaced-cards").querySelectorAll(".card"),node=>node.dataset.id)'),
    [second.id,row.id]);
  assert(await evaluate(`document.activeElement.matches(${JSON.stringify(title)})`));
}
