import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";
import { clickNode, until } from "./browser-actions.mjs";

const html = await fs.readFile(new URL("../index.html", import.meta.url), "utf8");
const project = "target:" + JSON.stringify(["remote:a", "targetServer", "canonical"]);
const identifier = "remote:" + JSON.stringify(["codex", "remote:a", "actual-uuid"]);
const fixture = {
  states: { unknown: "待判断", review: "待审阅", to_archive: "待归档" },
  health: { remoteRegistry: [{ hostId: "remote:a", hostLabel: "Mac", enabled: true }],
    remoteConnections: [{ hostId: "remote:a", connected: true, catalog: { cached: false } }] },
  projectCatalog: { loaded: true, items: [
    { id: project, nativeId: "canonical", hostId: "remote:a", hostLabel: "Mac",
      origin: "targetServer", remote: true, label: "项目", paths: ["/remote/project"], canCreate: true },
    { id: "foreign", hostId: "remote:b", hostLabel: "Other", origin: "targetServer",
      remote: true, label: "别的主机", paths: ["/remote/project"], canCreate: true },
  ] },
  sessions: [{ id: identifier, threadId: "actual-uuid", title: "刚创建的会话",
    remote: true, hostId: "remote:a", hostLabel: "Mac", cwd: "/remote/project",
    state: "review", note: "", ordinary: true, catalogCurrent: true, presence: "active",
    archived: false, projectId: null, version: 0, inputVersion: 1 }],
};
const writes = [];
let failAssignment = true, failNavigation = false, browser, cleanup, cases = 0;
const evidence = process.env.BOARD_SSH_PREVIEW_EVIDENCE;
const setupRequests = [];
const setupTarget = { hostname: "studio.example.test", user: "builder", port: "22", directory: "/srv/project" };
const setupSummary = "builder@studio.example.test:22 · /srv/project · csb-studio";
let setupDeclined = false, previewCount = 0;
let releaseAssignment;
const assignmentReady = new Promise(resolve => { releaseAssignment = resolve; });
let releaseRefresh;
const refreshReady = new Promise(resolve => { releaseRefresh = resolve; });
const check = (actual, expected) => { assert.deepEqual(actual, expected); cases++; };
function sshSetupResponse(body) {
  setupRequests.push(body);
  if (body.action === "status") return setupDeclined ? {
    state: "declined", targetId: "isolated-target", summary: setupSummary,
    message: "该连接的一键远程新建已关闭。已有会话操作仍可使用。",
  } : { state: "form" };
  if (body.action === "decline" && body.previewId === `preview-${previewCount}`) {
    setupDeclined = true;
    return { state: "declined", targetId: "isolated-target", summary: setupSummary,
      message: "该连接的一键远程新建已关闭。已有会话操作仍可使用。" };
  }
  if ((body.action === "preview" && !setupDeclined) ||
      (body.action === "restore" && setupDeclined && body.targetId === "isolated-target")) {
    setupDeclined = false;
    return { state: "preview", previewId: `preview-${++previewCount}`, summary: setupSummary, canApply: false,
      message: "隔离预览。实际写入功能尚未接入。",
      changes: [
        { label: "新增 ~/.ssh/codex-session-board.conf",
          detail: "+ Host csb-studio\n+     HostName studio.example.test\n+     User builder\n+     Port 22" },
        { label: "在 ~/.ssh/config 顶层加入 Include", detail: "+ Include ~/.ssh/codex-session-board.conf" },
        { label: "新增 Codex 连接、登记 /srv/project 并开启该连接的自动连接" },
      ] };
  }
  return { error: "此操作不可用" };
}
const server = http.createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST" && request.url === "/api/ssh-setup") {
    const result = sshSetupResponse(JSON.parse(body));
    return response.writeHead(result.error ? 409 : 200, { "Content-Type": "application/json" })
      .end(JSON.stringify(result));
  }
  if (request.method === "POST") {
    writes.push({ path: request.url, body: JSON.parse(body) });
    if (request.url === "/api/new" && failNavigation)
      return response.writeHead(200, { "Content-Type": "application/json" })
        .end(JSON.stringify({ pending: true, error: "输入页尚未确认" }));
    if (request.url === "/api/project/assign") await assignmentReady;
    if (request.url === "/api/project/assign" && failAssignment)
      return response.writeHead(409, { "Content-Type": "application/json" })
        .end(JSON.stringify({ error: "补登记未确认，请对同一会话重试" }));
    if (request.url === "/api/project/assign") fixture.sessions[0].projectId = project;
  }
  if (request.method === "GET" && request.url === "/api/sessions"
      && fixture.sessions[0].projectId === project) await refreshReady;
  response.writeHead(200, { "Content-Type": request.url === "/" ? "text/html; charset=utf-8" : "application/json" });
  response.end(request.url === "/" ? html : JSON.stringify(request.method === "POST" ? { ok: true } : fixture));
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
try {
  browser = await launchBrowser();
  const address = `http://127.0.0.1:${server.address().port}/`;
  const page = await browser.page(address);
  const evaluate = expression => page.evaluate(expression);
  const wait = expression => until(() => evaluate(expression), "页面状态未就绪：" + expression);
  const capture = async name => {
    if (!evidence) return;
    const image = await page.command("Page.captureScreenshot", { format: "png" });
    await fs.writeFile(`${evidence}/${name}.png`, Buffer.from(image.data, "base64"));
  };
  await wait('typeof data !== "undefined" && data.sessions.length === 1');
  await evaluate(`byId("host-filter").value="host:remote:a";byId("host-filter").dispatchEvent(new Event("change"));
    byId("project-filter").value=${JSON.stringify("project:" + project)};
    byId("project-filter").dispatchEvent(new Event("change"))`);
  check(await evaluate('canCreateProject(data.projectCatalog.items[0])'), true);
  await clickNode(page, "#new-session");
  await wait("!creating && byId('feedback').textContent.includes('发送第一句话后才会创建会话')");
  check(writes.length, 1);
  check(writes[0].path, "/api/new");
  check(writes[0].body.projectId, project);
  check(Object.keys(writes[0].body).sort(), ["operationId", "projectId"]);
  check(/^[0-9a-f-]{36}$/.test(writes[0].body.operationId), true);
  check(await evaluate('byId("feedback").textContent.includes("发送第一句话后才会创建会话")'), true);
  await capture("01-reuse");
  await evaluate('byId("project-filter").value="all";byId("project-filter").dispatchEvent(new Event("change"))');
  await clickNode(page, "[data-action=assign-project]");
  check(await evaluate('byId("project-assignment").open'), true);
  check(await evaluate('byId("assignment-thread").textContent.includes("actual-uuid")'), true);
  check(await evaluate('Array.from(byId("assignment-project").options).map(option=>option.value)'), [project]);
  check(await evaluate('byId("host-filter").disabled'), true);
  await evaluate('byId("assignment-form").requestSubmit()');
  await until(() => writes.length === 2, "补登记请求尚未到达");
  for (let attempt = 0; attempt < 2; attempt++) {
    for (const type of ["keyDown", "keyUp"])
      await page.command("Input.dispatchKeyEvent", { type, key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    check(await evaluate('byId("project-assignment").open'), true);
  }
  assert.equal(await evaluate('byId("project-assignment").open && saving'), true,
    JSON.stringify(await evaluate('({open:byId("project-assignment").open,saving,error:byId("assignment-error").textContent})')));
  cases++;
  releaseAssignment();
  await wait('!saving && byId("assignment-error").textContent.includes("补登记未确认")');
  check(await evaluate('byId("project-assignment").open'), true);
  check(await evaluate('data.sessions[0].id'), identifier);
  failAssignment = false;
  await evaluate('byId("assignment-form").requestSubmit()');
  await wait('!saving && !byId("project-assignment").open');
  check(await evaluate('document.querySelector("[data-action=assign-project]") !== null'), true);
  releaseRefresh();
  await wait(`data.sessions[0].projectId === ${JSON.stringify(project)}
    && document.querySelector("[data-action=assign-project]") === null`);
  check(writes.filter(write => write.path === "/api/project/assign").map(write => write.body),
    [{ id: identifier, projectId: project, confirmed: true }, { id: identifier, projectId: project, confirmed: true }]);
  check(writes.filter(write => write.path === "/api/new").length, 1);
  check(await evaluate('data.sessions.length'), 1);
  check(await evaluate('document.querySelector("[data-action=assign-project]") === null'), true);
  const legacy = "target:" + JSON.stringify(["remote:a", "targetLegacy", "old"]);
  fixture.projectCatalog.items.push({ ...fixture.projectCatalog.items[0], id: legacy, origin: "targetLegacy" });
  await evaluate("load()");
  failNavigation = true;
  await evaluate(`createSession(${JSON.stringify(legacy)})`);
  check(await evaluate('byId("draft-recovery").open'), true);
  const failedNavigation = writes.at(-1);
  fixture.projectCatalog.items = fixture.projectCatalog.items.filter(project => project.id !== legacy);
  await evaluate("load()");
  failNavigation = false;
  await evaluate('byId("draft-retry").click()');
  await wait("!creating");
  check(writes.at(-1), failedNavigation);
  check(await evaluate('byId("draft-recovery").open'), false);

  const registration = JSON.stringify(fixture.health.remoteRegistry);
  const checkpoints = writes.length;
  const oldConnectionReason = "这条 Codex 连接未绑定 SSH 别名。请在 Codex 侧栏选择原主机和项目，再新建会话。通过别名添加可能产生第二条连接。";
  Object.assign(fixture.projectCatalog.items[0], { canCreate: false, createError: oldConnectionReason });
  await evaluate("load()");
  await evaluate(`byId("project-filter").value=${JSON.stringify("project:" + project)};
    byId("project-filter").dispatchEvent(new Event("change"))`);
  await clickNode(page, "#new-session");
  check(await evaluate('byId("feedback").textContent'), oldConnectionReason);
  check(writes.length, checkpoints);
  check(JSON.stringify(fixture.health.remoteRegistry), registration);
  await capture("03-original-connection");

  fixture.health.sshAliasSetup = true;
  await evaluate("load()");
  await clickNode(page, "#ssh-settings");
  await wait('sshSetup?.state==="form" && !sshSetupBusy');
  await evaluate(`for (const [name,value] of Object.entries(${JSON.stringify(setupTarget)}))
    byId("ssh-setup-form").elements.namedItem(name).value=value`);
  await clickNode(page, "#ssh-setup-next");
  await wait('sshSetup?.state==="preview" && !sshSetupBusy');
  check(setupRequests.at(-1), { action: "preview", target: setupTarget });
  check(await evaluate('byId("ssh-setup-next").disabled'), true);
  check(await evaluate('byId("ssh-setup-changes").children.length'), 3);
  await capture("02-authorization-preview");
  await clickNode(page, "#ssh-setup-decline");
  await wait('sshSetup?.state==="declined" && !sshSetupBusy');
  check(setupRequests.at(-1), { action: "decline", previewId: "preview-1" });
  const reopened = await browser.page(address);
  await until(() => reopened.evaluate('document.getElementById("ssh-settings")?.hidden === false'), "连接设置按钮未就绪");
  await clickNode(reopened, "#ssh-settings");
  await until(() => reopened.evaluate('sshSetup?.state==="declined" && !sshSetupBusy'), "拒绝状态未读回");
  check(await reopened.evaluate('byId("ssh-setup-restore").hidden'), false);
  if (evidence) {
    const image = await reopened.command("Page.captureScreenshot", { format: "png" });
    await fs.writeFile(`${evidence}/02-declined.png`, Buffer.from(image.data, "base64"));
  }
  await clickNode(reopened, "#ssh-setup-restore");
  await until(() => reopened.evaluate('sshSetup?.state==="preview" && !sshSetupBusy'), "重新启用预览未读回");
  check(setupRequests.at(-1), { action: "restore", targetId: "isolated-target" });
  check(await reopened.evaluate('sshSetup.previewId'), "preview-2");
  check(writes.length, checkpoints);
  check(JSON.stringify(fixture.health.remoteRegistry), registration);
} finally {
  releaseAssignment();
  releaseRefresh();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({ passed: true, cases, cleanup }));
