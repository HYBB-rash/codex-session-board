/// <reference lib="webworker" />

declare const self: ServiceWorkerGlobalScope;

const unavailable = `<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>会话看板</title>
<style>:root{color-scheme:light dark}body{max-width:28rem;margin:15vh auto;padding:2rem;font:1rem/1.6 system-ui}</style>
<h1>会话看板离线</h1><p>请打开装有 Enhancer 的 Codex 窗口。</p>
<p>当前无法确认主机和会话状态。</p><a href="/">重新连接</a></html>`;

self.addEventListener("install", (event) => event.waitUntil(self.skipWaiting()));
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (event) => {
    const request = event.request;
    const url = new URL(request.url);
    if (
        request.method !== "GET" ||
        request.mode !== "navigate" ||
        url.origin !== self.location.origin ||
        url.pathname !== "/"
    )
        return;
    event.respondWith(
        fetch(request, { cache: "no-store" }).catch(
            () =>
                new Response(unavailable, {
                    status: 503,
                    headers: {
                        "Content-Type": "text/html; charset=utf-8",
                        "Cache-Control": "no-store",
                        "X-Content-Type-Options": "nosniff",
                    },
                }),
        ),
    );
});

export {};
