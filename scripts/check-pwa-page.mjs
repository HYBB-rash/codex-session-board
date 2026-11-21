import assert from "node:assert/strict";
import { launchBrowser } from "./browser-client.mjs";
import { clickNode, inputText, until } from "./browser-actions.mjs";

const origin = new URL(process.argv[2]).origin;
assert.equal(new URL(origin).hostname, "127.0.0.1");
const browser = await launchBrowser();
try {
  const page = await browser.page(origin);
  await until(() => page.evaluate("Boolean(navigator.serviceWorker.controller)"), "首页未受 Service Worker 控制");
  const assets = await page.evaluate(`(async () => {
    const manifest = document.querySelector('link[rel~="manifest"]');
    const installScript = [...document.scripts].find(script => script.src === location.origin + "/assets/pwa.js");
    const image = document.querySelector("#move-dragon");
    await image.decode();
    const registration = await navigator.serviceWorker.ready;
    const snapshot = await fetch("/api/sessions");
    return {manifest:manifest?.href, installScript:installScript?.defer || installScript?.type === "module",
      worker:registration.active.scriptURL, scope:registration.scope,
      embeddedImage:image.currentSrc.startsWith("data:image/webp;base64,") && image.naturalWidth > 0,
      loadedSessions:snapshot.ok && (await snapshot.json()).sessions.length > 0};
  })()`);
  assert.deepEqual(assets, {manifest:origin + "/manifest.webmanifest", installScript:true, worker:origin + "/sw.js",
    scope:origin + "/", embeddedImage:true, loadedSessions:true});
  const showFakePrompt = `failure => {
    const prompt = new Event("beforeinstallprompt", {cancelable:true});
    prompt.prompt = async () => {
      window.installCalls = (window.installCalls || 0) + 1;
      if (failure) throw new Error("fixture prompt failure");
    };
    prompt.userChoice = Promise.resolve({outcome:"dismissed"});
    window.dispatchEvent(prompt);
    const button = document.querySelector("#install-app");
    const shown = !button.hidden;
    button.click();
    return prompt.defaultPrevented && shown && button.hidden;
  }`;
  assert(await page.evaluate(`(${showFakePrompt})(true)`));
  await until(() => page.evaluate('document.querySelector("#feedback").textContent.includes("未能打开安装窗口")'),
    "安装提示失败后未显示浏览器菜单说明");
  assert(await page.evaluate(`(${showFakePrompt})(false)`));
  assert.equal(await page.evaluate("window.installCalls"), 2);
  const before = await (await fetch(origin + "/api/info")).json();
  assert(before.requestToken);

  const reachable = () => fetch(origin + "/api/info", {signal:AbortSignal.timeout(1000)})
    .then(response => response.ok, () => false);
  console.log("stop");
  await until(async () => !await reachable(), "隔离 HTTP 服务尚未停止");
  await page.command("Page.navigate", {url:origin + "/"});
  await until(() => page.evaluate('document.querySelector("h1")?.textContent === "本地看板服务未连接"'), "断线时未显示重新连接页");
  const offline = await page.evaluate(`(async () => ({
    status:performance.getEntriesByType("navigation")[0].responseStatus,
    reconnect:document.querySelector("a")?.href,
    cachedBoard:Boolean(await caches.match("/")),
    cachedSessions:Boolean(await caches.match("/api/sessions")),
    apiReachable:await fetch("/api/sessions").then(() => true, () => false)
  }))()`);
  assert.deepEqual(offline, {status:503, reconnect:origin + "/", cachedBoard:false, cachedSessions:false, apiReachable:false});
  console.log("start");
  await until(reachable, "隔离 HTTP 服务未恢复");
  await clickNode(page, 'a[href="/"]');
  await until(() => page.evaluate('Boolean(document.querySelector("#board-viewport"))'), "重新连接后未回到看板");
  const after = await (await fetch(origin + "/api/info")).json();
  assert.notEqual(after.pid, before.pid);
  assert(after.requestToken);
  assert.notEqual(after.requestToken, before.requestToken);
  const {sessions:[card]} = await (await fetch(origin + "/api/sessions")).json();
  const note = "PWA 重连后保存";
  const denied = await fetch(origin + "/api/state", {
    method:"POST", headers:{"Content-Type":"application/json", "Origin":origin, "X-CSRF-Token":before.requestToken},
    body:JSON.stringify({id:card.id,state:card.state,note,version:card.version,inputVersion:card.inputVersion,
      sectionVersion:card.sectionVersion}),
  });
  assert.equal(denied.status, 403);
  await until(() => page.evaluate(`Boolean(document.querySelector('[data-id="${card.id}"] [data-action="adjust"]:not(:disabled)'))`),
    "重连后未显示可编辑卡片");
  await clickNode(page, `[data-id="${card.id}"] [data-action="adjust"]`);
  await inputText(page, "#note", note);
  await clickNode(page, "#save");
  await until(async () => {
    const {sessions} = await (await fetch(origin + "/api/sessions")).json();
    return sessions.some(row => row.id === card.id && row.note === note && row.version > card.version);
  }, "重连后的页面令牌未能保存会话");
} finally {
  await browser.close();
}
