import assert from "node:assert/strict";

export async function checkCardDisplay(evaluate, snapshot, wait) {
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
  snapshot.sessions[0].observationWarning = "当前执行证据待确认";
  await evaluate("load()");
  await wait(`${card}.querySelector(".card-title").textContent === "当前卡片标题已更新"`);
  assert(await evaluate(`${card}.textContent.includes("当前执行证据待确认")`));
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
}
