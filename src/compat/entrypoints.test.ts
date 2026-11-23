import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import {
    mkdirSync,
    mkdtempSync,
    readFileSync,
    realpathSync,
    rmSync,
    statSync,
    utimesSync,
    writeFileSync,
} from "node:fs";
import { request as httpRequest } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { type TestContext, test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";
import { connectBoard } from "./client.js";
import { createMcpHandler, resourceUri } from "./mcp.js";
import { assetsDirectory, type BoardRequest, loadOperations } from "./operations.js";
import { readReceipt, writeReceipt } from "./receipt.js";
import { HostUnavailableError, startBoardServer } from "./server.js";

const exec = promisify(execFile);
const version = "test";
const state = {
    id: "thread",
    state: "review",
    note: "",
    version: 2,
    inputVersion: 3,
    sectionVersion: "live",
};
const projectCatalog = { loaded: true, items: [] };
const reviewBinding = { state: "review", label: "待审阅", sectionId: "test-native-section" };
const example = {
    states: { review: "待审阅" },
    health: { nativeVerified: true, sectionBindings: [reviewBinding] },
    projectCatalog,
    sessions: [{ id: "thread", title: "A session", canOpen: true }],
};
const byId = { id: "thread" };
const requestId = { projectId: "project", requestId: "request" };
const argumentsByAction: Record<string, object> = {
    Snapshot: {},
    Update: state,
    Sort: { state: "review", field: "updated_at", direction: "desc", version: 2 },
    Archive: { ...byId, version: 2, inputVersion: 3, batchId: "batch" },
    ArchiveBatches: {},
    ArchiveBatchStart: {
        id: "batch",
        items: [{ ...byId, title: "A session", version: 2, inputVersion: 3 }],
    },
    ArchiveBatchFinish: { id: "batch" },
    ArchiveBatchCheck: { id: "batch" },
    Open: byId,
    PreviewReply: byId,
    New: requestId,
    NewReset: { ...requestId, confirmed: true },
    ProjectAssign: { ...byId, projectId: "project", confirmed: true },
    Connect: { hostId: "remote-ssh-discovered:test" },
    SshSetup: { action: "status" },
    Refresh: {},
    PluginUpdateStatus: {},
    PluginUpdateStart: {},
};

function isolated(test: TestContext) {
    const directory = mkdtempSync(join(realpathSync(tmpdir()), "enhancer-compat-"));
    test.after(() => rmSync(directory, { force: true, recursive: true }));
    return directory;
}

async function fixture(test: TestContext, request: BoardRequest) {
    const directory = isolated(test);
    let account = "test-account";
    const server = await startBoardServer({
        stateDir: directory,
        port: 0,
        request,
        cacheAccount: () => account,
    });
    test.after(() => server.close());
    return {
        directory,
        ...server,
        switchAccount: (value: string) => {
            account = value;
        },
    };
}

function hostRequest(url: string, host: string): Promise<number | undefined> {
    return new Promise((resolve, reject) => {
        const request = httpRequest(url, { headers: { Host: host } }, (response) => {
            response.resume();
            resolve(response.statusCode);
        });
        request.on("error", reject);
        request.end();
    });
}

function post(url: string, path: string, payload: object, headers?: Record<string, string>) {
    return fetch(new URL(path, url), {
        method: "POST",
        body: JSON.stringify(payload),
        headers: { "Content-Type": "application/json", ...headers },
    });
}

test("HTTP and PWA use loopback, active origin and validated shared operations", async (test) => {
    const received: unknown[][] = [];
    const { url, directory } = await fixture(test, async (path, body) => {
        received.push([path, body]);
        return { path, body };
    });
    assert.equal(await hostRequest(url, "example.test"), 403);
    for (const path of ["/", "/board.js", "/sw.js", "/manifest.webmanifest", "/assets/board.svg"]) {
        const response = await fetch(new URL(path, url));
        assert.equal(response.status, 200, path);
        assert.equal(response.headers.get("Cache-Control"), "no-store");
        assert.ok((await response.text()).length > 100);
    }
    const info = (await (await fetch(new URL("/api/info", url))).json()) as {
        requestToken: string;
    };
    assert.equal(statSync(join(directory, "board-service.json")).mode & 0o777, 0o600);
    assert.equal((await post(url, "api/state", state)).status, 403);
    const headers = { "X-CSRF-Token": info.requestToken, origin: new URL(url).origin };
    assert.equal(
        (await post(url, "api/state", state, { ...headers, origin: "https://example.test" }))
            .status,
        403,
    );
    assert.equal(
        (await post(url, "api/state", { ...state, hostId: "wrong" }, headers)).status,
        400,
    );
    assert.equal((await post(url, "api/state", state, headers)).status, 200);
    assert.equal((await post(url, "api/service/stop", {}, headers)).status, 404);
    assert.deepEqual(received, [["/api/state", state]]);
});

test("offline account-specific snapshots never reissue writes or previews", async (test) => {
    let online = true;
    let updates = 0;
    const { url, switchAccount } = await fixture(test, async (path) => {
        if (!online) throw new HostUnavailableError();
        if (path === "/api/state") updates++;
        return example;
    });
    assert.deepEqual(await (await fetch(new URL("/api/sessions", url))).json(), example);
    online = false;
    const stale = (await (await fetch(new URL("/api/sessions", url))).json()) as {
        offline: boolean;
        sessions: { readOnly: boolean; canOpen: boolean }[];
    };
    assert.equal(stale.offline, true);
    assert.equal(stale.sessions[0]?.readOnly, true);
    assert.equal(stale.sessions[0]?.canOpen, false);
    const info = (await (await fetch(new URL("/api/info", url))).json()) as {
        requestToken: string;
    };
    const headers = { "X-CSRF-Token": info.requestToken };
    assert.equal((await post(url, "api/state", state, headers)).status, 503);
    assert.equal((await post(url, "api/preview-reply", { id: "thread" }, headers)).status, 503);
    assert.equal(updates, 0);
    switchAccount("new-account");
    assert.equal((await fetch(new URL("/api/sessions", url))).status, 503);
});

test("a pending offline response cannot fall back to the previous account's snapshot", async (test) => {
    let online = true;
    let requested!: () => void;
    let disconnected!: () => void;
    const entered = new Promise<void>((resolve) => {
        requested = resolve;
    });
    const released = new Promise<void>((resolve) => {
        disconnected = resolve;
    });
    const { url, switchAccount } = await fixture(test, async () => {
        if (online) return example;
        requested();
        await released;
        throw new HostUnavailableError();
    });
    const snapshot = new URL("/api/sessions", url);
    assert.deepEqual(await (await fetch(snapshot)).json(), example);
    online = false;
    const pending = fetch(snapshot);
    await entered;
    switchAccount("new-account");
    disconnected();
    const response = await pending;
    assert.equal(response.status, 409);
    assert.deepEqual(await response.json(), { error: "Codex 账号已变更，请重新读取看板" });
});

function mcp(request: BoardRequest) {
    const rpc = createMcpHandler({ request, version });
    return async (method: string, params?: object) => {
        const response = await rpc(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
        assert.ok(response && "result" in response);
        return response.result;
    };
}

function expectedToolResult(action: string, response: object) {
    return action === "PreviewReply"
        ? { content: [], _meta: { previewReply: response } }
        : {
              structuredContent: response,
              content: [{ type: "text", text: JSON.stringify(response) }],
          };
}

test("all 18 HTTP routes and MCP tools forward the same arguments and return the same result", async (test) => {
    const received: unknown[][] = [];
    const { directory, url } = await fixture(test, async (path, body) => {
        received.push([path, body]);
        return { path, body: body ?? null };
    });
    const operations = loadOperations();
    const answer = mcp((await connectBoard(directory)).request);
    const toolList = (await answer("tools/list")) as {
        tools: {
            name: string;
            _meta: { "openai/ui"?: { entrypoints: object[] }; ui: { visibility: string[] } };
        }[];
    };
    assert.deepEqual(
        toolList.tools.map(({ name }) => name),
        ["board_view", ...operations.map((operation) => operation.tool)],
    );
    assert.deepEqual(toolList.tools[0]?._meta["openai/ui"]?.entrypoints, [{ type: "global" }]);
    assert.deepEqual(
        Object.keys(argumentsByAction).sort(),
        operations.map(({ action }) => action).sort(),
    );
    const info = (await (await fetch(new URL("/api/info", url))).json()) as {
        requestToken: string;
    };
    const headers = { "X-CSRF-Token": info.requestToken, origin: new URL(url).origin };
    for (const [index, { action, method, path, tool }] of operations.entries()) {
        const payload = argumentsByAction[action];
        assert.ok(payload, action);
        const body = method === "POST" ? payload : undefined;
        const expected = { path, body: body ?? null };
        const http = body ? await post(url, path, body, headers) : await fetch(new URL(path, url));
        assert.equal(http.status, action === "Refresh" ? 202 : 200, action);
        assert.deepEqual(await http.json(), expected, action);
        assert.deepEqual(toolList.tools[index + 1]?._meta.ui.visibility, ["app"], action);
        const result = await answer("tools/call", { name: tool, arguments: payload });
        assert.deepEqual(result, expectedToolResult(action, expected), action);
        assert.deepEqual(received.splice(0), [
            [path, body],
            [path, body],
        ]);
    }
});

test("CLI mcp negotiates stdio, reads its packaged app and reports rejected saves", async (test) => {
    const { directory } = await fixture(test, async (path) => {
        if (path === "/api/state") throw new Error("状态版本过期");
        return example;
    });
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const requests = [
        { method: "initialize", params: { protocolVersion: "2025-11-25" } },
        { method: "resources/read", params: { uri: resourceUri } },
        { method: "tools/call", params: { name: "board_update", arguments: state } },
    ];
    const output = await new Promise<string>((resolve, reject) => {
        const processHandle = execFile(
            process.execPath,
            [binary, "--state-dir", directory, "mcp"],
            { timeout: 10_000, maxBuffer: 4 * 1024 * 1024 },
            (error, stdout) => (error ? reject(error) : resolve(stdout)),
        );
        processHandle.stdin?.end(
            requests
                .map((request, index) => JSON.stringify({ jsonrpc: "2.0", id: index, ...request }))
                .join("\n"),
        );
    });
    const responses = output
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line));
    assert.deepEqual(
        responses.map((response) => response.id),
        [0, 1, 2],
    );
    assert.equal(responses[0].result.protocolVersion, "2025-11-25");
    const resource = responses[1].result.contents[0];
    assert.match(resource.text, /<script>/);
    assert.doesNotMatch(resource.text, /<(?:iframe|script)\b[^>]*\bsrc=/i);
    assert.equal(resource.mimeType, "text/html;profile=mcp-app");
    assert.deepEqual(responses[2].result, {
        isError: true,
        content: [{ type: "text", text: "状态版本过期" }],
    });
});

test("CLI reuses the authorized instance and stop only closes its compatibility service", async (test) => {
    let value = example;
    const { directory, url } = await fixture(test, async () => value);
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const run = (action: string) =>
        exec(process.execPath, [binary, "--state-dir", directory, action]);
    assert.deepEqual(JSON.parse((await run("serve")).stdout), {
        url,
        running: true,
        started: false,
        config: null,
    });
    assert.deepEqual(JSON.parse((await run("scan")).stdout), example);
    const checked = JSON.parse((await run("check-sections")).stdout);
    assert.equal(checked.nativeCatalogVerified, true);
    assert.deepEqual(checked.bindings, [reviewBinding]);
    value = { ...example, health: { nativeVerified: true, sectionBindings: [] } };
    await assert.rejects(run("check-sections"), /本机原生分区绑定无效/);
    const running = readReceipt(directory);
    assert.ok(running);
    const wrongInstance = await post(
        url,
        "api/service/stop",
        { instance: "retired" },
        { Authorization: `Bearer ${running.clientToken}` },
    );
    assert.equal(wrongInstance.status, 409);
    const wrongProcess = await post(
        url,
        "api/service/stop",
        { instance: running.instance, pid: running.pid + 1 },
        { Authorization: `Bearer ${running.clientToken}` },
    );
    assert.equal(wrongProcess.status, 409);
    assert.equal(JSON.parse((await run("stop")).stdout).stopped, true);
    assert.equal(readReceipt(directory), null);
    await assert.rejects(run("scan"), /请打开装有 Enhancer/);
});

test("CLI stop waits for the verified service to drain before reporting success", async (test) => {
    let finish!: () => void;
    const active = new Promise<void>((resolve) => {
        finish = resolve;
    });
    let markReceived!: () => void;
    const received = new Promise<void>((resolve) => {
        markReceived = resolve;
    });
    const { directory, url } = await fixture(test, async () => {
        markReceived();
        await active;
        return example;
    });
    const client = await connectBoard(directory);
    const snapshot = client.request("/api/sessions");
    await received;
    const receipt = readReceipt(directory);
    assert.ok(receipt);
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const stopping = exec(process.execPath, [binary, "--state-dir", directory, "stop"]);
    const stopResult = stopping.then(
        () => "stopped",
        () => "failed",
    );
    try {
        let accepting = true;
        for (let attempt = 0; attempt < 50 && accepting; attempt += 1) {
            accepting = await fetch(`${url}api/info`).then(
                async (response) => {
                    await response.text();
                    return true;
                },
                () => false,
            );
            if (accepting) await delay(20);
        }
        assert.equal(accepting, false, "the listener has stopped accepting connections");
        assert.equal(readReceipt(directory)?.instance, receipt.instance);
        assert.equal(await Promise.race([stopResult, delay(80, "draining")]), "draining");
    } finally {
        finish();
    }
    assert.deepEqual(await snapshot, example);
    assert.deepEqual(JSON.parse((await stopping).stdout), {
        running: false,
        stopped: true,
        instance: receipt.instance,
    });
    assert.equal(readReceipt(directory), null);
});

test("an unreachable live host keeps its state directory and cannot be replaced", async (test) => {
    const { directory, close } = await fixture(test, async () => example);
    const receipt = readReceipt(directory);
    assert.ok(receipt);
    await close();
    const occupied = await fixture(test, async () => example);
    const { processStart, ...legacyReceipt } = receipt;
    assert.ok(processStart);
    for (const live of [receipt, legacyReceipt]) {
        writeReceipt(directory, live);
        await assert.rejects(connectBoard(directory), HostUnavailableError);
        await assert.rejects(
            startBoardServer({
                stateDir: directory,
                port: Number(new URL(occupied.url).port),
                request: async () => example,
            }),
            /已有看板服务实例/,
        );
        assert.deepEqual(readReceipt(directory), live);
        rmSync(join(directory, "board-service.json"));
    }
});

test("a reused process ID does not hold a stale board receipt", async (test) => {
    const { directory, close } = await fixture(test, async () => example);
    const receipt = readReceipt(directory);
    assert.ok(receipt?.processStart);
    await close();
    const target = join(directory, "board-service.json");
    const previousStart = receipt.processStart.replace(/\d+$/, "0");
    for (const stale of [
        { ...receipt, processStart: previousStart },
        { ...receipt, processStart: undefined },
    ]) {
        writeFileSync(target, JSON.stringify(stale), { mode: 0o600 });
        utimesSync(target, new Date(0), new Date(0));
        await assert.rejects(connectBoard(directory), HostUnavailableError);
        const replacement = await startBoardServer({
            stateDir: directory,
            port: 0,
            request: async () => example,
        });
        try {
            assert.notEqual(readReceipt(directory)?.instance, receipt.instance);
            assert.deepEqual(
                await (await connectBoard(directory)).request("/api/sessions"),
                example,
            );
        } finally {
            await replacement.close();
        }
    }
});

test("a client bound to a stopped instance cannot close its replacement", async (test) => {
    const { directory, url, close } = await fixture(test, async () => example);
    const original = await connectBoard(directory);
    await close();
    const port = Number(new URL(url).port);
    writeFileSync(join(directory, "config.json"), JSON.stringify({ port }));
    const replacement = await startBoardServer({
        stateDir: directory,
        request: async () => example,
    });
    test.after(() => replacement.close());
    const current = await connectBoard(directory);
    const receipt = readReceipt(directory);
    await assert.rejects(original.stop(), /看板服务实例已变更/);
    assert.deepEqual(await current.request("/api/sessions"), example);
    assert.deepEqual(readReceipt(directory), receipt);
});

test("migrate-sections retains the source and refuses to overwrite an existing output", async (test) => {
    const directory = isolated(test);
    const source = join(directory, "old.json");
    const output = join(directory, "output.json");
    const input = JSON.stringify({
        native_sections: true,
        data_dir: "saved",
        status_sections: { completed: "old", review: "live" },
    });
    writeFileSync(source, input);
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const run = () =>
        exec(process.execPath, [
            binary,
            "--config",
            source,
            "migrate-sections",
            "--output",
            output,
        ]);
    assert.deepEqual(JSON.parse((await run()).stdout).removedStates, ["completed"]);
    assert.equal(readFileSync(source, "utf8"), input);
    assert.deepEqual(JSON.parse(readFileSync(output, "utf8")).status_sections, { review: "live" });
    await assert.rejects(run(), /EEXIST/);
});

test("CLI finds the receipt in the actual Enhancer state layout", async (test) => {
    const directory = isolated(test);
    const service = await startBoardServer({
        stateDir: join(directory, "session-board"),
        port: 0,
        request: async () => example,
    });
    test.after(() => service.close());
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const result = await exec(process.execPath, [binary, "--state-dir", directory, "scan"]);
    assert.deepEqual(JSON.parse(result.stdout), example);
});

test("CLI migrate imports a live SQLite WAL and refuses an existing destination", async (test) => {
    const directory = isolated(test);
    const source = join(directory, "old");
    const config = join(directory, "old.json");
    const output = join(directory, "new");
    mkdirSync(source);
    writeFileSync(config, JSON.stringify({ data_dir: source }));
    const database = new DatabaseSync(join(source, "status.sqlite3"));
    test.after(() => database.close());
    database.exec(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE fixture (note TEXT)",
    );
    database.prepare("INSERT INTO fixture VALUES (?)").run("saved");
    const binary = join(assetsDirectory, "..", "bin", "codex-session-board.js");
    const run = () =>
        exec(process.execPath, [
            binary,
            "--state-dir",
            directory,
            "migrate",
            "--source-config",
            config,
            "--output",
            output,
        ]);
    const migrated = JSON.parse((await run()).stdout);
    assert.equal(migrated.receipt.mainDatabase, true);
    assert.equal(migrated.directory, join(output, "snapshot"));
    const copy = new DatabaseSync(join(migrated.directory, "status.sqlite3"), { readOnly: true });
    test.after(() => copy.close());
    const { note: copiedNote } = copy.prepare("SELECT note FROM fixture").get() ?? {};
    assert.equal(copiedNote, "saved");
    await assert.rejects(run(), /已经存在/);
    const { note: originalNote } = database.prepare("SELECT note FROM fixture").get() ?? {};
    assert.equal(originalNote, "saved");
});
