import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { promisify } from "node:util";

const execute = promisify(execFile);
const browser =
    process.env.BOARD_TEST_BROWSER ??
    (process.platform === "darwin"
        ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        : "google-chrome");
const asset = await readFile(new URL("../dist/board-assets/mcp.html", import.meta.url));

function initialCard() {
    const id = "00000000-0000-4000-8000-000000000001";
    return {
        id,
        threadId: id,
        hostId: "local",
        title: "验收会话",
        state: "review",
        note: "验收前备注",
        version: 3,
        inputVersion: 2,
        sectionVersion: "original-placement",
        readOnly: false,
        canOpen: true,
        presence: "present",
    };
}

function boardHost(frame, result) {
    let session = initialCard();
    const calls = [];
    const snapshot = () => ({
        states: {
            in_progress: "进行中",
            review: "待审阅",
            blocked: "已阻塞",
            to_archive: "待归档",
        },
        sessions: [session],
        health: { connected: true, nativeVerified: true },
        projectCatalog: { loaded: true, items: [] },
    });
    const background = {
        board_snapshot: snapshot,
        board_refresh: snapshot,
        board_plugin_update_status: () => ({ available: false }),
        board_archive_batches: () => ({ batches: [] }),
    };
    const previewReply = { turnId: "fixture-turn", html: "<p>完整回复</p>", inProgress: false };
    const operations = {
        board_preview_reply: () => ({ content: [], _meta: { previewReply } }),
        board_open: () => ({ content: [], structuredContent: { ok: true } }),
        board_update: ({ state, note }) => {
            session = { ...session, state, note, version: session.version + 1 };
            return { content: [], structuredContent: { ok: true, session } };
        },
    };
    window.addEventListener(
        "message",
        messageHandler(frame, result, (name, args) => {
            if (Object.hasOwn(background, name))
                return { content: [], structuredContent: background[name]() };
            calls.push({ name, args });
            if (Object.hasOwn(operations, name)) return operations[name](args);
        }),
    );
    return { calls, current: () => session };
}

function messageHandler(frame, result, runTool) {
    return ({ data, source, origin }) => {
        if (source !== frame.contentWindow || origin !== location.origin || data?.jsonrpc !== "2.0")
            return;
        if (data.id === undefined) return;
        let response;
        if (data.method === "ui/initialize") {
            response = {
                protocolVersion: data.params.protocolVersion,
                hostInfo: { name: "acceptance", version: "1" },
                hostCapabilities: { serverTools: {} },
                hostContext: { theme: "light" },
            };
        } else if (data.method === "tools/call") {
            const { name, arguments: args } = data.params;
            response = runTool(name, args);
        }
        if (response) source.postMessage({ jsonrpc: "2.0", id: data.id, result: response }, origin);
        else {
            result.textContent = `unexpected RPC ${data.method} ${data.params?.name ?? ""}`;
            source.postMessage(
                {
                    jsonrpc: "2.0",
                    id: data.id,
                    error: { code: -32601, message: "unexpected request" },
                },
                origin,
            );
        }
    };
}

function requireInBrowser(condition, message) {
    if (!condition) throw new Error(message);
}

function sleep(duration) {
    return new Promise((done) => setTimeout(done, duration));
}

async function until(read, message) {
    for (let attempt = 0; attempt < 90; attempt++) {
        const value = read();
        if (value) return value;
        await sleep(40);
    }
    throw new Error(message);
}

function pageControls(frame) {
    const child = frame.contentWindow;
    const transfer = new child.DataTransfer();
    const find = (selector) => frame.contentDocument?.querySelector(selector);
    const drag = (element, kind) =>
        element.dispatchEvent(
            new child.DragEvent(kind, { bubbles: true, cancelable: true, dataTransfer: transfer }),
        );
    const pointerClick = (element) =>
        element.dispatchEvent(
            new child.MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 }),
        );
    const title = () => find('article.card button[data-action="preview"]');
    return { child, transfer, find, drag, pointerClick, title };
}

async function checkNavigation({ title, pointerClick, find, drag }, { calls, current }) {
    const stillClosed = () => calls.length === 0 && find("#reply-reader").hidden;
    pointerClick(title());
    await sleep(80);
    requireInBrowser(stillClosed(), "dragging called preview or navigation");
    drag(title(), "dragend");
    pointerClick(title());
    await sleep(65);
    requireInBrowser(stillClosed(), "post-drag click opened the card");
    await sleep(450);
    find('article.card button[data-action="adjust"]').click();
    await until(() => find("#editor[open]"), "adjust control did not open its form");
    requireInBrowser(stillClosed(), "adjust action also opened preview");
    find("#cancel").click();
    await until(() => !find("#editor[open]"), "cancel did not close the form");
    pointerClick(title());
    await until(
        () => find("#reply-text")?.textContent === "完整回复",
        "title did not open reply preview",
    );
    requireInBrowser(
        calls.length === 1 && calls[0].name === "board_preview_reply",
        "preview also navigated",
    );
    requireInBrowser(calls[0].args.id === current().id, "preview used the wrong card");
    find("#reply-open").click();
    await until(() => calls.length === 2, "explicit open control did not navigate");
    requireInBrowser(
        calls[1].name === "board_open" && calls[1].args.id === current().id,
        "open targeted the wrong card",
    );
}

function assertSavedCard({ name, args }, original) {
    requireInBrowser(
        name === "board_update" &&
            args.id === original.id &&
            args.state === "blocked" &&
            args.note === "" &&
            args.version === original.version &&
            args.inputVersion === original.inputVersion &&
            args.sectionVersion === original.sectionVersion,
        "explicit save did not carry the edited card and revisions",
    );
}

async function checkBlocked({ find, drag, child }, { calls, current }) {
    const initial = current();
    const destination = find('#board .column[data-state="blocked"] .column-heading');
    requireInBrowser(destination, "blocked column is absent");
    drag(destination, "dragover");
    drag(destination, "drop");
    const dialog = await until(() => find("#editor[open]"), "blocked drop did not open the editor");
    const note = dialog.querySelector("#note");
    const hint = dialog.querySelector(`#${note.getAttribute("aria-describedby")}`)?.textContent;
    requireInBrowser(
        dialog.querySelector("#state").value === "blocked",
        "blocked state was not selected",
    );
    requireInBrowser(note.value === initial.note, "opened editor lost the saved note");
    requireInBrowser(
        !note.required && /请说明.*不填也可以保存/u.test(hint ?? ""),
        "no guidance that the reason is optional",
    );
    note.value = "";
    note.dispatchEvent(new child.Event("input", { bubbles: true }));
    await sleep(80);
    requireInBrowser(dialog.querySelector("form").checkValidity(), "blank reason prevents saving");
    requireInBrowser(
        calls.length === 0 && current() === initial,
        "opening or editing the form saved state or notes",
    );
    const column = () => find("article.card")?.closest(".column")?.dataset.state;
    requireInBrowser(column() === initial.state, "unsaved card moved");
    dialog.querySelector("#save").click();
    await until(
        () => calls.length === 1 && !find("#editor[open]"),
        "explicit save did not complete",
    );
    assertSavedCard(calls[0], initial);
    await until(() => column() === "blocked", "saved card did not move");
}

async function fixture(scope) {
    const result = document.querySelector("output");
    const frame = document.querySelector("iframe");
    const host = boardHost(frame, result);
    frame.src = "/mcp.html";
    const controls = pageControls(frame);
    try {
        const title = await until(controls.title, "installed MCP card is absent");
        const shell = controls.find(".shell");
        const style = frame.contentWindow.getComputedStyle(shell);
        requireInBrowser(
            style.display === "flex" && style.getPropertyValue("--bg"),
            "offline board styles are absent",
        );
        controls.drag(title, "dragstart");
        requireInBrowser(
            controls.transfer.getData("text/plain") === host.current().id,
            "title did not initiate a card drag",
        );
        await until(() => controls.find(".drag-active"), "board did not enter drag state");
        await (scope === "navigation" ? checkNavigation : checkBlocked)(controls, host);
        if (result.textContent === "waiting") result.textContent = "passed";
    } catch (error) {
        result.textContent = `${error.message}; tools: ${host.calls.map(({ name }) => name).join(", ")}`;
    }
}

async function check(context, scope) {
    const source = [
        initialCard,
        boardHost,
        messageHandler,
        requireInBrowser,
        sleep,
        until,
        pageControls,
        checkNavigation,
        assertSavedCard,
        checkBlocked,
        fixture,
    ].join("\n");
    const html = `<!doctype html><meta charset="utf-8"><title>Board interaction fixture</title>
<output>waiting</output><iframe style="width:100%;height:720px;border:0"></iframe>
<script>${source}\nfixture(${JSON.stringify(scope)})</script>`;
    const server = createServer((request, response) => {
        if (request.url === "/")
            return response.setHeader("Content-Type", "text/html; charset=utf-8").end(html);
        if (request.url === "/mcp.html")
            return response.setHeader("Content-Type", "text/html; charset=utf-8").end(asset);
        return response.writeHead(404).end();
    });
    await new Promise((ready, fail) => server.once("error", fail).listen(0, "127.0.0.1", ready));
    context.after(() => new Promise((done) => server.close(done)));
    const profile = await mkdtemp(join(tmpdir(), "board-interaction-"));
    context.after(() => rm(profile, { recursive: true, force: true }));
    const { stdout } = await execute(
        browser,
        [
            "--headless",
            "--dump-dom",
            "--no-proxy-server",
            "--virtual-time-budget=8000",
            `--user-data-dir=${profile}`,
            `http://127.0.0.1:${server.address().port}/`,
        ],
        { timeout: 20_000 },
    );
    assert.equal(stdout.match(/<output>([^<]*)<\/output>/u)?.[1], "passed");
}

test("dragging the card title suppresses preview and navigation without disabling nearby controls", (context) =>
    check(context, "navigation"));
test("the blocked editor accepts an optional reason and sends no update until explicit save", (context) =>
    check(context, "blocked"));
