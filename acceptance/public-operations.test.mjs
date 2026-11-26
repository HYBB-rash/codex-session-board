import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { Script } from "node:vm";
import { connectBoard } from "../dist/compat/client.js";
import { createMcpHandler } from "../dist/compat/mcp.js";
import { startBoardServer } from "../dist/compat/server.js";
import { labels } from "../dist/extensions/placement.js";
import { createBoardFixture } from "./typescript-fixture.mjs";

const require = createRequire(import.meta.url);
globalThis.window = { setTimeout, clearTimeout };

function isolated(context) {
    const directory = mkdtempSync(join(tmpdir(), "board-public-acceptance-"));
    context.after(() => rmSync(directory, { recursive: true, force: true }));
    return directory;
}

function fixtureTurn({ laterTurn, incomplete }) {
    return {
        id: laterTurn ? "round-two" : "round-one",
        status: laterTurn ? "completed" : "inProgress",
        itemsView: incomplete ? "notLoaded" : "summary",
        startedAt: laterTurn ? Date.now() / 1000 + 1 : 1001,
        items: incomplete
            ? []
            : [
                  { id: "input-one", type: "userMessage" },
                  { id: "input-two", type: "userMessage" },
              ],
    };
}

function createPublicBoardFixture(directory, states) {
    const controls = {
        commands: [],
        nativeMethods: [],
        newInput: false,
        incomplete: false,
        laterTurn: false,
        rejectMove: false,
    };

    function transport(packet, desktop, forward) {
        const { hostId, request } = packet;
        controls.nativeMethods.push(request.method);
        if ((controls.newInput || controls.incomplete) && request.method === "thread/turns/list") {
            return desktop.receive({
                type: "mcp-response",
                hostId,
                message: {
                    id: request.id,
                    result: { data: [fixtureTurn(controls)], nextCursor: null },
                },
            });
        }
        return forward(packet, desktop);
    }

    function sidebar(command, forward) {
        controls.commands.push(command);
        if (command.kind === "move" && controls.rejectMove)
            throw new Error("fixture move rejected");
        return forward(command);
    }

    function edit(state, note) {
        const { id, version, inputVersion, sectionVersion } = board.card();
        return { id, state, note, version, inputVersion, sectionVersion };
    }

    const board = createBoardFixture(directory, states, { transport, sidebar, lazyDeferred: true });
    return {
        get service() {
            return board.service;
        },
        reopen: board.reopen,
        nativeMove: board.nativeMove,
        step: board.step,
        card: board.card,
        persisted: board.persisted,
        controls,
        edit,
    };
}

async function liveBoard(context) {
    const directory = isolated(context);
    const board = createPublicBoardFixture(directory, Object.entries(labels));
    await board.step("running", 1000);
    const server = await startBoardServer({
        stateDir: directory,
        port: 0,
        request: (route, body) => board.service.request(route, body),
        cacheAccount: () => "acceptance-account",
    });
    context.after(() => server.close());
    const client = await connectBoard(directory);
    const rpc = createMcpHandler({ request: client.request, version: "acceptance" });
    const mcpUpdate = async (payload) => {
        const response = await rpc(
            JSON.stringify({
                jsonrpc: "2.0",
                id: 1,
                method: "tools/call",
                params: { name: "board_update", arguments: payload },
            }),
        );
        assert.equal(response.id, 1);
        assert.ok("result" in response);
        return response.result;
    };
    return { ...server, directory, board, client, mcpUpdate };
}

function assertRejected(result) {
    assert.deepEqual(Object.keys(result).sort(), ["content", "isError"]);
    assert.equal(result.isError, true);
    assert.match(result.content[0]?.text, /卡片位置或输入已更新/);
}

function changes(board) {
    const [records] = Object.values(JSON.parse(board.persisted().files[0]));
    return records[board.card().id]?.changes ?? [];
}

function packageMcp(directory) {
    const root = new URL("../", import.meta.url);
    const plugin = JSON.parse(readFileSync(new URL(".codex-plugin/plugin.json", root)));
    const { mcpServers } = JSON.parse(readFileSync(new URL(plugin.mcpServers, root)));
    const server = mcpServers.session_board;
    assert.equal(server.command, "node");
    assert.ok(server.env_vars.includes("CODEX_ENHANCER_STATE_DIR"));
    return {
        icon: readFileSync(new URL(plugin.interface.composerIcon, root)).toString("base64"),
        async call(requests) {
            const output = await new Promise((resolve, reject) => {
                const child = execFile(
                    process.execPath,
                    server.args,
                    {
                        cwd: new URL(server.cwd, root),
                        env: { ...process.env, CODEX_ENHANCER_STATE_DIR: directory },
                        timeout: 10_000,
                        maxBuffer: 4 * 1024 * 1024,
                    },
                    (error, stdout) => (error ? reject(error) : resolve(stdout)),
                );
                child.stdin.end(
                    requests
                        .map(([method, params], id) =>
                            JSON.stringify({ jsonrpc: "2.0", id, method, params }),
                        )
                        .join("\n"),
                );
            });
            const responses = output.trim().split("\n").map(JSON.parse);
            assert.equal(responses.length, requests.length);
            return responses.map((response, index) => {
                assert.equal(response.id, index);
                assert.ok("result" in response, JSON.stringify(response));
                return response.result;
            });
        },
    };
}

test("the MCP plugin serves the board page without a global entrypoint", async (context) => {
    const { board, directory, url } = await liveBoard(context);
    const mcp = packageMcp(directory);
    const [tools, listed] = await mcp.call([["tools/list"], ["resources/list"]]);
    assert.equal(
        tools.tools.some((tool) =>
            tool._meta?.["openai/ui"]?.entrypoints?.some(({ type }) => type === "global"),
        ),
        false,
    );
    const view = tools.tools.find(({ name }) => name === "board_view");
    assert.ok(view);
    assert.equal(view.title, "会话看板");
    assert.equal(view.annotations.readOnlyHint, true);
    assert.deepEqual(view._meta.ui.visibility, ["app", "model"]);
    assert.deepEqual(view.icons[0], {
        src: `data:image/svg+xml;base64,${mcp.icon}`,
        mimeType: "image/svg+xml",
    });
    const resourceUri = view._meta.ui.resourceUri;
    assert.deepEqual(listed.resources, [
        { uri: resourceUri, name: "会话看板", mimeType: "text/html;profile=mcp-app" },
    ]);
    const [published, opened, snapshot] = await mcp.call([
        ["resources/read", { uri: resourceUri }],
        ["tools/call", { name: view.name }],
        ["tools/call", { name: "board_snapshot" }],
    ]);
    const [page] = published.contents;
    assert.equal(page.uri, resourceUri);
    assert.equal(page.mimeType, listed.resources[0].mimeType);
    assert.equal(page._meta["openai/ui"].preferredDisplayMode, "fullscreen");
    assert.equal(opened.structuredContent.ready, true);
    assert.deepEqual(snapshot.structuredContent.sessions, [board.card()]);
    const html = await fetch(url);
    assert.equal(html.status, 200);
    assert.match(await html.text(), /<script src="\/board\.js" defer>/);
    const script = await fetch(new URL("/board.js", url));
    assert.equal(script.status, 200);
    const webPage = await script.text();
    for (const content of [page.text, webPage]) {
        for (const landmark of ["board-viewport", "reply-reader", "edit-form"]) {
            assert.ok(content.includes(landmark), landmark);
        }
    }
});

test("MCP App and the web page read each other's saved board state", async (context) => {
    const { board, directory, url } = await liveBoard(context);
    const mcp = packageMcp(directory);
    const call = (name, args) => ["tools/call", { name, arguments: args }];
    const sessionFromHTTP = async () => {
        const response = await fetch(new URL("/api/sessions", url));
        assert.equal(response.status, 200);
        const { sessions } = await response.json();
        assert.equal(sessions.length, 1);
        return sessions[0];
    };
    const initial = await sessionFromHTTP();
    const { requestToken } = await (await fetch(new URL("/api/info", url))).json();
    const response = await fetch(new URL("/api/state", url), {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "X-CSRF-Token": requestToken,
            Origin: new URL(url).origin,
        },
        body: JSON.stringify(board.edit("review", "网页保存的备注")),
    });
    assert.equal(response.status, 200);
    const webSaved = (await response.json()).session;
    assert.equal(webSaved.id, initial.id);
    assert.equal(webSaved.version, initial.version + 1);
    assert.equal(webSaved.inputVersion, initial.inputVersion);
    assert.equal(webSaved.state, "review");
    assert.equal(webSaved.note, "网页保存的备注");
    const [readBack, saved] = await mcp.call([
        call("board_snapshot"),
        call("board_update", board.edit("blocked", "MCP 保存的备注")),
    ]);
    assert.deepEqual(readBack.structuredContent.sessions, [webSaved]);
    assert.equal(saved.structuredContent.ok, true);
    const mcpSaved = saved.structuredContent.session;
    assert.equal(mcpSaved.id, initial.id);
    assert.equal(mcpSaved.version, webSaved.version + 1);
    assert.equal(mcpSaved.inputVersion, webSaved.inputVersion);
    assert.equal(mcpSaved.state, "blocked");
    assert.equal(mcpSaved.note, "MCP 保存的备注");
    board.reopen();
    await board.service.refresh();
    assert.deepEqual(await sessionFromHTTP(), mcpSaved);
});

test("the MCP resource bundles its SDK and styles without linked scripts or frames", async () => {
    const rpc = createMcpHandler({
        request: () => assert.fail("unexpected write"),
        version: "acceptance",
    });
    const response = await rpc(
        JSON.stringify({
            jsonrpc: "2.0",
            id: 1,
            method: "resources/read",
            params: { uri: "ui://session-board/index.html" },
        }),
    );
    assert.equal(response.id, 1);
    assert.ok("result" in response);
    const [resource] = response.result.contents;
    assert.equal(resource.mimeType, "text/html;profile=mcp-app");
    assert.deepEqual(resource._meta.ui.csp, {
        connectDomains: [],
        resourceDomains: [],
        frameDomains: [],
    });
    const scriptTag = /<script(?:\s([^>]*))?>([\s\S]+?)<\/script>/gi;
    const scripts = [...resource.text.matchAll(scriptTag)];
    assert.equal(scripts.length, 1);
    const [, attributes, script] = scripts[0];
    assert.equal(attributes, undefined);
    const document = resource.text.replaceAll(scriptTag, "");
    const external = /<(?:link|iframe|frame|object|embed)\b|<(?:script|img)\b[^>]*\bsrc=/i;
    assert.equal(external.test(document), false, "resource contains a linked asset or frame");
    const styles = [...resource.text.matchAll(/<style(?:\s[^>]*)?>([\s\S]+?)<\/style>/g)];
    assert.match(script, /\.board-page\s*\{/u, "offline stylesheet missing");
    assert.ok(styles.every(([, css]) => !/@import\b|url\(\s*['"]?(?:https?:|\/\/)/i.test(css)));
    assert.doesNotThrow(() => new Script(script));
    for (const signal of ["ui/initialize", "ui/notifications/initialized", "postMessage"]) {
        assert.equal(script.includes(signal), true, signal);
    }
});

test("MCP saves use the live state and input revisions across HTTP", async (context) => {
    const { board, client, mcpUpdate } = await liveBoard(context);
    const stale = board.edit("blocked", "stale");
    const saved = await client.request("/api/state", board.edit("review", "saved"));
    assert.equal(saved.ok, true);
    assert.equal(saved.session.version, stale.version + 1);
    assert.equal(saved.session.inputVersion, stale.inputVersion);
    assert.equal(saved.session.note, "saved");
    const before = board.persisted();
    assertRejected(await mcpUpdate(stale));
    assert.deepEqual(board.persisted(), before);
    const outdatedInput = board.edit("blocked", "would overwrite");
    board.controls.newInput = true;
    assertRejected(await mcpUpdate(outdatedInput));
    const after = board.card();
    assert.equal(after.version, outdatedInput.version + 1);
    assert.equal(after.inputVersion, outdatedInput.inputVersion + 1);
    assert.equal(after.note, "saved");
    assert.equal(after.state, "review");
    assert.equal(after.manual, true);
    assert.equal(after.followUp, true);
    assert.equal(board.persisted().writes, before.writes);
    await assert.rejects(client.request("/api/state", outdatedInput), /卡片位置或输入已更新/);
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().note, "saved");
    assert.equal(board.card().followUp, true);
});

test("deferral requires a fresh reason and follows the confirmed native placement", async (context) => {
    const { board, client, mcpUpdate } = await liveBoard(context);
    const edit = board.edit("deferred", "已有备注");
    assert.equal(board.card().canDefer, true);
    assert.ok(
        !board.service.snapshot().health.sectionBindings.some(({ state }) => state === "deferred"),
    );
    const before = board.persisted();
    await assert.rejects(client.request("/api/state", edit), /请填写搁置原因/);
    const blank = await mcpUpdate({ ...edit, deferReason: "　\t\n" });
    assert.equal(blank.isError, true);
    assert.match(blank.content[0].text, /请填写搁置原因/);
    assert.deepEqual(board.persisted(), before);

    const deferred = await mcpUpdate({ ...edit, deferReason: " 等回复 " });
    assert.equal(deferred.structuredContent.session.state, "deferred");
    assert.equal(deferred.structuredContent.session.deferReason, "等回复");
    assert.equal(deferred.structuredContent.session.note, "已有备注");
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().deferReason, "等回复");
    const old = board.edit("deferred");
    const enteredAt = JSON.parse(old.sectionVersion)[2];
    await board.nativeMove("review", enteredAt);
    await board.nativeMove("deferred", enteredAt);
    assert.equal(board.card().deferReason, null);
    assert.equal(board.card().note, "已有备注");
    await assert.rejects(
        client.request("/api/state", { ...old, deferReason: "过期理由" }),
        /卡片位置或输入已更新/,
    );
    await assert.rejects(client.request("/api/state", board.edit("deferred")), /请填写搁置原因/);
    const checkpoint = board.persisted().writes;
    const filled = await client.request("/api/state", {
        ...board.edit("deferred"),
        deferReason: "等新的回复",
    });
    assert.equal(filled.session.deferReason, "等新的回复");
    assert.equal(board.persisted().writes, checkpoint);
    await board.step("input", 1010);
    assert.equal(board.card().state, "blocked");
    assert.match(board.card().reason, /等待你回答/);
    assert.equal(board.card().deferReason, undefined);
    assert.equal(board.card().note, "已有备注");
});

test("new-input notices survive incomplete observations and clear only on manual confirmation", async (context) => {
    const { board, client } = await liveBoard(context);
    const { session } = await client.request("/api/state", board.edit("review", "keep my note"));
    const fresh = board.edit("review", "keep my note");
    board.controls.newInput = true;
    board.controls.laterTurn = true;
    await assert.rejects(client.request("/api/state", fresh), /卡片位置或输入已更新/);
    const pending = board.card();
    assert.equal(pending.inputVersion, session.inputVersion + 1);
    assert.equal(pending.followUp, true);
    board.controls.incomplete = true;
    await board.service.refresh();
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().inputVersion, pending.inputVersion);
    assert.equal(board.card().followUp, true);
    assert.equal(board.card().manual, true);
    assert.equal(board.card().state, "review");
    assert.equal(board.card().note, "keep my note");
    board.controls.incomplete = false;
    const { session: confirmed } = await client.request("/api/state", board.edit("review"));
    assert.equal(confirmed.inputVersion, pending.inputVersion);
    assert.equal(confirmed.followUp, false);
    assert.equal(confirmed.note, "keep my note");
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().followUp, false);
});

test("disabling title sync still saves state and never renames", async (context) => {
    const { board, mcpUpdate } = await liveBoard(context);
    const saved = await mcpUpdate(board.edit("in_progress", "keep my note"));
    assert.equal(saved.isError, undefined);
    assert.equal(saved.structuredContent.ok, true);
    assert.equal(saved.structuredContent.titleEnabled, false);
    assert.equal(saved.structuredContent.session.state, "in_progress");
    assert.equal(saved.structuredContent.session.note, "keep my note");
    const moved = await mcpUpdate(board.edit("blocked"));
    assert.equal(moved.structuredContent.session.state, "blocked");
    assert.equal(moved.structuredContent.session.note, "keep my note");
    assert.equal(board.card().title, "验收会话");
    assert.equal(board.card().titleSyncPending, undefined);
    assert.equal(board.controls.nativeMethods.includes("thread/name/set"), false);
    assert.equal(board.controls.commands.filter(({ kind }) => kind === "move").length, 2);
});

test("manual confirmations append durable change records and rejected saves leave them alone", async (context) => {
    const { board, client, mcpUpdate } = await liveBoard(context);
    const stale = board.edit("review", "initial");
    const { session: first } = await client.request("/api/state", stale);
    const { structuredContent: second } = await mcpUpdate(board.edit("blocked"));
    assert.equal(second.session.note, "initial");
    const { session: third } = await client.request("/api/state", board.edit("blocked", "　\t\n"));
    assert.equal(third.note, "");
    assert.deepEqual(
        changes(board)
            .filter(({ kind }) => kind === "manual")
            .map(({ changedAt, ...change }) => {
                assert.ok(Number.isFinite(changedAt));
                return change;
            }),
        [
            { kind: "manual", state: "review", note: "initial", version: first.version },
            { kind: "manual", state: "blocked", note: "initial", version: second.session.version },
            { kind: "manual", state: "blocked", note: "", version: third.version },
        ],
    );
    const committed = changes(board);
    assertRejected(await mcpUpdate(stale));
    board.controls.rejectMove = true;
    await assert.rejects(
        client.request("/api/state", board.edit("review", "failed")),
        /fixture move rejected/,
    );
    const afterFailure = changes(board);
    assert.deepEqual(afterFailure, committed);
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().state, "blocked");
    assert.equal(board.card().note, "");
    assert.deepEqual(changes(board), committed);
});

test("confirmed automatic phases append their reasons once without replacing the saved note", async (context) => {
    const { board, client } = await liveBoard(context);
    await client.request("/api/state", board.edit("review", "keep"));
    const count = changes(board).length;
    await board.step("input", 1010);
    const committed = changes(board);
    assert.equal(committed.length, count + 1);
    const latest = committed.at(-1);
    assert.equal(latest.kind, "automatic");
    assert.equal(latest.state, "blocked");
    assert.equal(latest.note, board.card().reason);
    assert.ok(latest.note);
    assert.equal(board.card().note, "keep");
    await board.step("input", 1011);
    board.reopen();
    await board.service.refresh();
    assert.deepEqual(changes(board), committed);
});

test("only loopback and same-origin token-authorized writes reach the board", async (context) => {
    const { url, board } = await liveBoard(context);
    const origin = new URL(url).origin;
    assert.equal(new URL(url).hostname, "127.0.0.1");
    const info = await fetch(`${origin}/api/info`).then((response) => response.json());
    assert.equal(typeof info.requestToken, "string");
    const previous = board.persisted();
    const post = (headers) =>
        fetch(`${origin}/api/state`, {
            method: "POST",
            headers: { "Content-Type": "application/json", ...headers },
            body: JSON.stringify(board.edit("review", "requested")),
        });
    for (const headers of [
        { Origin: origin },
        { Origin: origin, "X-CSRF-Token": "different-token" },
        { Origin: "https://external.example", "X-CSRF-Token": info.requestToken },
    ]) {
        const response = await post(headers);
        assert.equal(response.status, 403);
        await response.text();
        assert.deepEqual(board.persisted(), previous);
    }
    const accepted = await post({ Origin: origin, "X-CSRF-Token": info.requestToken });
    assert.equal(accepted.status, 200);
    assert.equal((await accepted.json()).session.note, "requested");
});

test("the client launcher passes the argument array without a shell", (context) => {
    const directory = isolated(context);
    const fs = require("node:fs");
    const childProcess = require("node:child_process");
    const installation = require("../dist/installer/install.js");
    const detection = require("../dist/installer/detect.js");
    const paths = require("../dist/installer/paths.js");
    const startScript = join(directory, "start client");
    const resourcesDir = join(directory, "resources");
    const target = { path: directory };
    setEnvironment(context, "CODEX_CLI_PATH", undefined);
    setEnvironment(context, "CODEX_ENHANCER_BRIDGE_SOCKET", join(directory, "fixture.sock"));
    setEnvironment(context, "CODEX_ENHANCER_LOG_STDOUT", "0");
    context.mock.method(installation, "readState", () => ({ status: "installed", target }));
    context.mock.method(detection, "locateApp", () => ({
        appDir: directory,
        startScript,
        binary: join(directory, "ChatGPT"),
        resourcesDir,
    }));
    context.mock.method(fs, "existsSync", () => true);
    context.mock.method(fs, "mkdirSync", () => undefined);
    const spawn = context.mock.method(childProcess, "spawn", () => ({ pid: 91, unref() {} }));
    const { launch } = require("../dist/installer/launch.js");
    const argumentsToPreserve = ["a path with spaces", "$literal", "single'quote"];
    assert.equal(launch(argumentsToPreserve), 91);
    assert.equal(spawn.mock.calls.length, 1);
    const [executable, args, options] = spawn.mock.calls[0].arguments;
    assert.equal(executable, startScript);
    assert.deepEqual(args, [`--user-data-dir=${paths.userData}`, ...argumentsToPreserve]);
    assert.equal(options.cwd, directory);
    assert.ok(options.shell === false || options.shell === undefined);
    if (process.platform === "linux") {
        assert.equal(options.env.CODEX_CLI_PATH, join(resourcesDir, "codex"));
    }
});

function setEnvironment(context, name, value) {
    const previous = process.env[name];
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
    context.after(() => {
        if (previous === undefined) delete process.env[name];
        else process.env[name] = previous;
    });
}
