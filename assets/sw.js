const unavailable = `<!doctype html>
<html lang="zh-CN"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="theme-color" content="#32694b"><title>会话看板 · 服务未连接</title>
<style>
body{margin:0;background:#f4f3ef;color:#242821;font:16px/1.7 system-ui,sans-serif}
main{max-width:480px;margin:15vh auto;padding:32px}h1{font-size:26px}
a{display:inline-block;background:#32694b;color:white;padding:8px 20px;border-radius:8px;text-decoration:none}
</style>
<main><h1>本地看板服务未连接</h1>
<p>请先启动这台电脑上的会话看板服务，再重新连接。</p>
<p>会话和备注保存在本地服务中，当前无法读取或保存。</p>
<a href="/">重新连接</a></main></html>`;

self.addEventListener("install", event => event.waitUntil(self.skipWaiting()));
self.addEventListener("activate", event => event.waitUntil(self.clients.claim()));
self.addEventListener("fetch", event => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || request.mode !== "navigate" ||
      url.origin !== self.location.origin || url.pathname !== "/") return;
  event.respondWith(fetch(request, {cache: "no-store"}).catch(() =>
    new Response(unavailable, {status: 503, headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff"
    }})));
});
