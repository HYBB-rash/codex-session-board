import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";

export async function until(operation, message, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await operation()) return;
    await delay(50);
  }
  throw new Error(message);
}

export async function point(page, selector) {
  return page.evaluate(`(()=>{
    const node=document.querySelector(${JSON.stringify(selector)});if(!node)throw Error("元素未找到");
    node.scrollIntoView({block:"center",inline:"nearest",behavior:"instant"});
    const bounds=node.getBoundingClientRect();
    return {x:bounds.left+bounds.width/2,y:bounds.top+Math.min(bounds.height/2,40)};
  })()`);
}

export async function clickNode(page, selector) {
  const position = await point(page, selector);
  await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...position, button: "left", buttons: 1, clickCount: 1 });
  await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...position, button: "left", clickCount: 1 });
}

export async function dragCard(page, id, state) {
  await page.command("Input.setInterceptDrags", { enabled: true });
  try {
    const start = await point(page, `.card[data-id="${id}"] .card-title`);
    const end = await point(page, `#column-${state} .cards`);
    const intercepted = page.event("Input.dragIntercepted");
    intercepted.catch(() => {});
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", ...start });
    await page.command("Input.dispatchMouseEvent", { type: "mousePressed", ...start, button: "left", buttons: 1, clickCount: 1 });
    await page.command("Input.dispatchMouseEvent", { type: "mouseMoved", x: start.x + 30, y: start.y + 15, buttons: 1, button: "left" });
    const { data } = await intercepted;
    assert(data.items.some(item => item.mimeType === "text/plain" && item.data === id));
    for (const type of ["dragEnter", "dragOver", "drop"])
      await page.command("Input.dispatchDragEvent", { type, ...end, data });
    await page.command("Input.dispatchMouseEvent", { type: "mouseReleased", ...end, button: "left", clickCount: 1 });
  } finally { await page.command("Input.setInterceptDrags", { enabled: false }); }
}

export async function inputText(page, selector, text) {
  await page.evaluate(`(()=>{
    const node=document.querySelector(${JSON.stringify(selector)});node.focus();node.select();
  })()`);
  if (text) await page.command("Input.insertText", { text });
  else {
    for (const type of ["keyDown", "keyUp"])
      await page.command("Input.dispatchKeyEvent", { type, key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8 });
  }
}
