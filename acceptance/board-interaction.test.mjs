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

function boardHost(frame, result, scope) {
    const canDefer = scope !== "missing-deferred";
    const unsupportedRemoteHosts =
        scope === "remote-control"
            ? [
                  {
                      hostId: "remote-control:fixture",
                      hostLabel: "测试 MacBook",
                      reason: "本版本暂不支持通过 Remote Control 管理远程任务，请使用 SSH 连接。",
                  },
              ]
            : undefined;
    let session = { ...initialCard(), canDefer, canArchive: true };
    const calls = [];
    const snapshot = () => ({
        states: {
            in_progress: "进行中",
            review: "待审阅",
            deferred: "待处理",
            blocked: "已阻塞",
        },
        sessions: [session],
        archivedSessions: session.archived ? [session] : [],
        health: { connected: true, nativeVerified: true, unsupportedRemoteHosts },
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
        async board_archive() {
            await sleep(350);
            session = { ...session, archived: true, canRestore: true };
            return { content: [], structuredContent: { item: { status: "archived" } } };
        },
        board_archive_read: () => ({
            content: [],
            _meta: {
                archivedConversation: {
                    turns: [
                        {
                            id: "fixture-turn",
                            messages: [
                                {
                                    id: "answer",
                                    text: "归档历史中的完整回复",
                                    role: "agentMessage",
                                },
                            ],
                        },
                    ],
                },
            },
        }),
        board_archive_restore: () => {
            session = { ...session, archived: false };
            return { content: [], structuredContent: { restored: true } };
        },
        board_update: ({ state, note = session.note, deferReason }) => {
            session = { ...session, state, note, deferReason, version: session.version + 1 };
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
    return async ({ data, source, origin }) => {
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
            response = await runTool(name, args);
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
            args.state === "deferred" &&
            args.note === original.note &&
            args.deferReason === "等回复" &&
            args.version === original.version &&
            args.inputVersion === original.inputVersion &&
            args.sectionVersion === original.sectionVersion,
        "explicit save did not carry the edited card and revisions",
    );
}

async function checkDeferral({ find, drag, child, title }, { calls, current }) {
    const initial = current();
    const drop = (state) => {
        const destination = find(`#board .column[data-state="${state}"] .column-heading`);
        requireInBrowser(destination, `${state} column is absent`);
        drag(destination, "dragover");
        drag(destination, "drop");
    };
    const open = async () => {
        drop("deferred");
        return until(() => find("#editor[open]"), "deferred drop did not open the editor");
    };
    let dialog = await open();
    const column = () => find("article.card")?.closest(".column")?.dataset.state;
    const setReason = (value) => {
        const field = dialog.querySelector("#defer-reason");
        field.value = value;
        field.dispatchEvent(new child.Event("input", { bubbles: true }));
    };
    requireInBrowser(dialog.querySelector("#state").value === "deferred", "wrong target");
    requireInBrowser(dialog.querySelector("#note").value === initial.note, "lost the saved note");
    requireInBrowser(!dialog.querySelector("form").checkValidity(), "empty reason is accepted");
    setReason("　 ");
    dialog.querySelector("#save").click();
    await until(() => find("#notice")?.textContent, "whitespace reason is accepted");
    requireInBrowser(
        calls.length === 0 && current() === initial,
        "whitespace reason saved state or notes",
    );
    requireInBrowser(column() === initial.state, "unsaved card moved");
    dialog.querySelector("#cancel").click();
    await until(() => !find("#editor[open]"), "cancel did not close the form");
    requireInBrowser(calls.length === 0 && column() === initial.state, "cancel saved the card");
    drag(title(), "dragstart");
    dialog = await open();
    setReason(" 等回复 ");
    dialog.querySelector("#save").click();
    await until(
        () => calls.length === 1 && !find("#editor[open]"),
        "explicit save did not complete",
    );
    assertSavedCard(calls[0], initial);
    await until(() => column() === "deferred", "saved card did not move");
    requireInBrowser(
        find("article.card").textContent.includes("搁置原因：等回复"),
        "reason absent",
    );
    drag(title(), "dragstart");
    drop("blocked");
    await until(() => calls.length === 2 && column() === "blocked", "blocked drop did not save");
    requireInBrowser(!find("#editor[open]"), "blocked drop opened a reason form");
    requireInBrowser(current().note === initial.note, "blocked drop lost the saved note");
}

async function checkMissingDeferredSection({ find, drag, transfer }, { calls, current }) {
    const initial = current();
    const destination = find("#column-deferred .column-heading");
    requireInBrowser(destination, "deferred column is absent");
    Object.defineProperty(transfer, "dropEffect", { value: "none", writable: true });
    const cancelled = !drag(destination, "dragover");
    requireInBrowser(cancelled && transfer.dropEffect === "move", "browser would suppress drop");
    drag(destination, "drop");
    await until(
        () => find("#feedback.error")?.textContent.includes("「待处理」分区"),
        "missing sidebar section was not reported",
    );
    requireInBrowser(!find("#editor[open]"), "opened an editor for an unavailable destination");
    requireInBrowser(
        calls.length === 0 && current() === initial,
        "changed the card without a section",
    );
}

async function checkRemoteControl({ find }) {
    const notice = find("#health-summary")?.textContent;
    requireInBrowser(
        notice?.includes("暂不支持通过 Remote Control") && notice.includes("请使用 SSH"),
        "board does not surface the SSH requirement",
    );
    find("#health-more").click();
    await until(() => find("#health-details[open]"), "connection details did not open");
    const host = find('[data-key="remote-control:fixture"]');
    requireInBrowser(host?.textContent.includes("测试 MacBook"), "missing unsupported host");
    requireInBrowser(!host.querySelector("button"), "unsupported host offered a connect action");
}

async function checkArchive({ find }, { calls, current }) {
    const button = find('.card-title-row [data-action="archive"]');
    requireInBrowser(
        button?.getAttribute("aria-label") === "归档会话",
        "missing title archive button",
    );
    const column = button.closest(".column");
    button.click();
    button.click();
    await until(() => find(".archive-check.spinning:disabled"), "no per-card archive progress");
    requireInBrowser(column.contains(find(".card")), "card moved before native confirmation");
    await until(
        () => current().archived && !find(".card"),
        "confirmed archived card is still visible",
    );
    requireInBrowser(
        calls.filter(({ name }) => name === "board_archive").length === 1,
        "duplicate write",
    );
    find("#archive-history-open").click();
    const row = `[data-archive-id="${current().id}"]`;
    const search = find(".archive-history input");
    search.value = "no matching record";
    search.dispatchEvent(new Event("input", { bubbles: true }));
    await until(() => !find(row), "archive search didn't filter");
    search.value = current().title.slice(0, 3);
    search.dispatchEvent(new Event("input", { bubbles: true }));
    await until(() => find(row), "archive search didn't find the same card");
    find(`${row} button:first-child`).click();
    await until(
        () => find(".archive-transcript")?.textContent.includes("归档历史中的完整回复"),
        "archived content missing",
    );
    find(".archive-history > button").click();
    await until(() => find(`${row} button:last-child`), "return to archive list failed");
    find(`${row} button:last-child`).click();
    await until(() => !find(row) && find(".card"), "restored card didn't return");
}

async function fixture(scope) {
    const result = document.querySelector("output");
    const frame = document.querySelector("iframe");
    const host = boardHost(frame, result, scope);
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
        if (scope === "archive") {
            await checkArchive(controls, host);
            result.textContent = "passed";
            return;
        }
        controls.drag(title, "dragstart");
        requireInBrowser(
            controls.transfer.getData("text/plain") === host.current().id,
            "title did not initiate a card drag",
        );
        await until(() => controls.find(".drag-active"), "board did not enter drag state");
        const checks = {
            navigation: checkNavigation,
            deferral: checkDeferral,
            "missing-deferred": checkMissingDeferredSection,
            "remote-control": checkRemoteControl,
        };
        await checks[scope](controls, host);
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
        checkDeferral,
        checkMissingDeferredSection,
        checkRemoteControl,
        checkArchive,
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
test("deferral requires a reason while blocked drag saves directly", (context) =>
    check(context, "deferral"));
test("dropping onto deferred reports a missing native section without changing the card", (context) =>
    check(context, "missing-deferred"));
test("the board explains that Remote Control requires switching to SSH", (context) =>
    check(context, "remote-control"));
test("a card stays put while archiving, then can be found, opened and restored in history", (context) =>
    check(context, "archive"));
