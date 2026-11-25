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
import { labels } from "../dist/extensions/session-board/placement.js";
import { BoardFixture } from "./typescript-fixture.mjs";

const require = createRequire(import.meta.url);
globalThis.window = { setTimeout, clearTimeout };

function isolated(context) {
    const directory = mkdtempSync(join(tmpdir(), "board-public-acceptance-"));
    context.after(() => rmSync(directory, { recursive: true, force: true }));
    return directory;
}

class PublicBoardFixture extends BoardFixture {
    commands = [];
    nativeMethods = [];
    newInput = false;
    incomplete = false;
    laterTurn = false;
    rejectMove = false;

    transport(packet, desktop) {
        const { hostId, request } = packet;
        this.nativeMethods.push(request.method);
        if ((this.newInput || this.incomplete) && request.method === "thread/turns/list") {
            return desktop.receive({
                type: "mcp-response",
                hostId,
                message: {
                    id: request.id,
                    result: {
                        data: [
                            {
                                id: this.laterTurn ? "round-two" : "round-one",
                                status: this.laterTurn ? "completed" : "inProgress",
                                itemsView: this.incomplete ? "notLoaded" : "summary",
                                startedAt: this.laterTurn ? Date.now() / 1000 + 1 : 1001,
                                items: this.incomplete
                                    ? []
                                    : [
                                          { id: "input-one", type: "userMessage" },
                                          { id: "input-two", type: "userMessage" },
                                      ],
                            },
                        ],
                        nextCursor: null,
                    },
                },
            });
        }
        return super.transport(packet, desktop);
    }

    sidebar(command) {
        this.commands.push(command);
        if (command.kind === "move" && this.rejectMove) throw new Error("fixture move rejected");
        return super.sidebar(command);
    }

    edit(state, note) {
        const { id, version, inputVersion, sectionVersion } = this.card();
        return { id, state, note, version, inputVersion, sectionVersion };
    }
}

async function liveBoard(context) {
    const directory = isolated(context);
    const board = new PublicBoardFixture(directory, Object.entries(labels));
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

test("the installed MCP plugin declares one global entrypoint for the complete board page", async (context) => {
    const { board, directory, url } = await liveBoard(context);
    const mcp = packageMcp(directory);
    const [tools, listed] = await mcp.call([["tools/list"], ["resources/list"]]);
    const global = tools.tools.filter((tool) =>
        tool._meta?.["openai/ui"]?.entrypoints?.some(({ type }) => type === "global"),
    );
    assert.equal(global.length, 1);
    const [view] = global;
    assert.equal(view.name, "board_view");
    assert.equal(view.title, "会话看板");
    assert.equal(view.annotations.readOnlyHint, true);
    assert.deepEqual(view._meta.ui.visibility, ["app", "model"]);
    assert.deepEqual(view._meta["openai/ui"].entrypoints, [{ type: "global" }]);
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
        for (const landmark of ["board-viewport", "reply-reader", "board-editor"]) {
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
        body: JSON.stringify(board.edit("to_archive", "网页保存的备注")),
    });
    assert.equal(response.status, 200);
    const webSaved = (await response.json()).session;
    assert.equal(webSaved.id, initial.id);
    assert.equal(webSaved.version, initial.version + 1);
    assert.equal(webSaved.inputVersion, initial.inputVersion);
    assert.equal(webSaved.state, "to_archive");
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
    assert.ok(
        styles.some(([, css]) => css.includes(":root")),
        "offline stylesheet missing",
    );
    assert.ok(styles.every(([, css]) => !/@import\b|url\(\s*['"]?(?:https?:|\/\/)/i.test(css)));
    assert.doesNotThrow(() => new Script(script));
    for (const signal of ["ui/initialize", "ui/notifications/initialized", "postMessage"]) {
        assert.equal(script.includes(signal), true, signal);
    }
});

test("MCP saves use the live state and input revisions across HTTP", async (context) => {
    const { board, client, mcpUpdate } = await liveBoard(context);
    const stale = board.edit("blocked", "stale");
    const saved = await client.request("/api/state", board.edit("to_archive", "saved"));
    assert.equal(saved.ok, true);
    assert.equal(saved.session.version, stale.version + 1);
    assert.equal(saved.session.inputVersion, stale.inputVersion);
    assert.equal(saved.session.note, "saved");
    const before = board.persisted();
    assertRejected(await mcpUpdate(stale));
    assert.deepEqual(board.persisted(), before);
    const outdatedInput = board.edit("blocked", "would overwrite");
    board.newInput = true;
    assertRejected(await mcpUpdate(outdatedInput));
    const after = board.card();
    assert.equal(after.version, outdatedInput.version + 1);
    assert.equal(after.inputVersion, outdatedInput.inputVersion + 1);
    assert.equal(after.note, "saved");
    assert.equal(after.state, "to_archive");
    assert.equal(after.manual, true);
    assert.equal(after.followUp, true);
    assert.equal(board.persisted().writes, before.writes);
    await assert.rejects(client.request("/api/state", outdatedInput), /卡片位置或输入已更新/);
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().note, "saved");
    assert.equal(board.card().followUp, true);
});

test("new-input notices survive incomplete observations and clear only on manual confirmation", async (context) => {
    const { board, client } = await liveBoard(context);
    const { session } = await client.request(
        "/api/state",
        board.edit("to_archive", "keep my note"),
    );
    const fresh = board.edit("to_archive", "keep my note");
    board.newInput = true;
    board.laterTurn = true;
    await assert.rejects(client.request("/api/state", fresh), /卡片位置或输入已更新/);
    const pending = board.card();
    assert.equal(pending.inputVersion, session.inputVersion + 1);
    assert.equal(pending.followUp, true);
    board.incomplete = true;
    await board.service.refresh();
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().inputVersion, pending.inputVersion);
    assert.equal(board.card().followUp, true);
    assert.equal(board.card().manual, true);
    assert.equal(board.card().state, "to_archive");
    assert.equal(board.card().note, "keep my note");
    board.incomplete = false;
    const { session: confirmed } = await client.request("/api/state", board.edit("to_archive"));
    assert.equal(confirmed.inputVersion, pending.inputVersion);
    assert.equal(confirmed.followUp, false);
    assert.equal(confirmed.note, "keep my note");
    board.reopen();
    await board.service.refresh();
    assert.equal(board.card().followUp, false);
});

test("disabling title sync still saves state and never renames", async (context) => {
    const { board, mcpUpdate } = await liveBoard(context);
    const saved = await mcpUpdate(board.edit("to_archive", "keep my note"));
    assert.equal(saved.isError, undefined);
    assert.equal(saved.structuredContent.ok, true);
    assert.equal(saved.structuredContent.titleEnabled, false);
    assert.equal(saved.structuredContent.session.state, "to_archive");
    assert.equal(saved.structuredContent.session.note, "keep my note");
    const moved = await mcpUpdate(board.edit("blocked"));
    assert.equal(moved.structuredContent.session.state, "blocked");
    assert.equal(moved.structuredContent.session.note, "keep my note");
    assert.equal(board.card().title, "验收会话");
    assert.equal(board.card().titleSyncPending, undefined);
    assert.equal(board.nativeMethods.includes("thread/name/set"), false);
    assert.equal(board.commands.filter(({ kind }) => kind === "move").length, 2);
});

test("manual confirmations append durable change records and rejected saves leave them alone", async (context) => {
    const { board, client, mcpUpdate } = await liveBoard(context);
    const stale = board.edit("to_archive", "initial");
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
            { kind: "manual", state: "to_archive", note: "initial", version: first.version },
            { kind: "manual", state: "blocked", note: "initial", version: second.session.version },
            { kind: "manual", state: "blocked", note: "", version: third.version },
        ],
    );
    const committed = changes(board);
    assertRejected(await mcpUpdate(stale));
    board.rejectMove = true;
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
    await client.request("/api/state", board.edit("to_archive", "keep"));
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
