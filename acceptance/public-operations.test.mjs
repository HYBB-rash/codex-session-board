import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
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
    return { ...server, board, client, mcpUpdate };
}

function assertRejected(result) {
    assert.deepEqual(Object.keys(result).sort(), ["content", "isError"]);
    assert.equal(result.isError, true);
    assert.match(result.content[0]?.text, /卡片位置或输入已更新/);
}

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
    const target = { path: directory };
    setEnvironment(context, "CODEX_ENHANCER_BRIDGE_SOCKET", join(directory, "fixture.sock"));
    setEnvironment(context, "CODEX_ENHANCER_LOG_STDOUT", "0");
    context.mock.method(installation, "readState", () => ({ status: "installed", target }));
    context.mock.method(detection, "locateApp", () => ({ startScript }));
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
});

function setEnvironment(context, name, value) {
    const previous = process.env[name];
    process.env[name] = value;
    context.after(() => {
        if (previous === undefined) delete process.env[name];
        else process.env[name] = previous;
    });
}
