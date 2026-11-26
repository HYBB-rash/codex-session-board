import assert from "node:assert/strict";
import test from "node:test";
import { codexPage, parseLaunch, parsePort } from "./launch.js";

test("the debugging port must be a loopback-safe TCP port", () => {
    assert.equal(parsePort("59578\n/devtools/browser/abc"), 59578);
    assert.equal(parsePort("0\n"), null);
    assert.equal(parsePort("65536\n"), null);
    assert.equal(parsePort("not-a-port\n"), null);
    assert.equal(parsePort(""), null);
});

test("only the Codex app page is accepted as a debugging target", () => {
    const page = {
        type: "page",
        id: "page-1",
        url: "app://-/index.html#/settings",
        webSocketDebuggerUrl: "ws://127.0.0.1:59578/devtools/page/page-1",
    };
    assert.deepEqual(codexPage(page), {
        id: "page-1",
        url: "app://-/index.html#/settings",
        webSocketDebuggerUrl: "ws://127.0.0.1:59578/devtools/page/page-1",
    });
    assert.equal(codexPage({ ...page, type: "worker" }), null);
    assert.equal(codexPage({ ...page, url: "https://example.com" }), null);
    assert.equal(codexPage({ ...page, webSocketDebuggerUrl: undefined }), null);
    assert.equal(codexPage(null), null);
});

test("the launch receipt requires every recorded identity", () => {
    const launch = {
        appDir: "/Applications/ChatGPT.app",
        binary: "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT",
        pid: 42,
        dataDir: "/Users/example/Library/Application Support/Codex",
        startedAt: "2026-11-26T00:00:00.000Z",
    };
    assert.deepEqual(parseLaunch(JSON.stringify(launch)), launch);
    assert.equal(parseLaunch(JSON.stringify({ ...launch, pid: "42" })), null);
    assert.equal(parseLaunch(""), null);
});
