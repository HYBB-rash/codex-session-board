import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http from "node:http";
import { launchBrowser } from "./browser-client.mjs";
import { clickNode, until } from "./browser-actions.mjs";

const html = await fs.readFile(new URL("../index.html", import.meta.url), "utf8");
const project = "target:" + JSON.stringify(["remote:a", "targetServer", "canonical"]);
const otherProject = "target:" + JSON.stringify(["remote:b", "targetServer", "canonical"]);
const identifier = "remote:" + JSON.stringify(["codex", "remote:a", "actual-uuid"]);
const fixture = {
  states: { unknown: "待判断", review: "待审阅", to_archive: "待归档" },
  health: { remoteRegistry: ["remote:a", "remote:b"].map(hostId => ({ hostId, hostLabel: "Mac", enabled: true })),
    remoteConnections: ["remote:a", "remote:b"].map(hostId => ({ hostId, connected: true, catalog: { cached: false } })) },
  projectCatalog: { loaded: true, items: [
    { id: project, nativeId: "canonical", hostId: "remote:a", hostLabel: "Mac",
      origin: "targetServer", remote: true, label: "项目", paths: ["/remote/project"], canCreate: true },
    { id: otherProject, nativeId: "canonical", hostId: "remote:b", hostLabel: "Mac", origin: "targetServer",
      remote: true, label: "项目", paths: ["/remote/project"], canCreate: true },
  ] },
  sessions: [{ id: identifier, threadId: "actual-uuid", title: "刚创建的会话",
    remote: true, hostId: "remote:a", hostLabel: "Mac", cwd: "/remote/project",
    state: "review", note: "", ordinary: true, catalogCurrent: true, presence: "active",
    archived: false, projectId: null, version: 0, inputVersion: 1, readOnly: false, canArchive: true }],
};
const writes = [];
const consentPreviews = [];
let failAssignment = true, failNavigation = false, browser, cleanup, cases = 0;
const evidence = process.env.BOARD_SSH_PREVIEW_EVIDENCE;
const setupRequests = [];
const setupTarget = { hostname: "studio.example.test", user: "builder", port: "22", directory: "/srv/project" };
const setupSummary = "builder@studio.example.test:22 · /srv/project · csb-studio";
let setupDeclined = false, previewCount = 0;
let requireConsent = false;
const declinedResult = () => ({ state: "declined", targetId: "isolated-target", summary: setupSummary,
  message: "该连接的一键远程新建已关闭。已有会话操作仍可使用。" });
let releaseAssignment;
const assignmentReady = new Promise(resolve => { releaseAssignment = resolve; });
let releaseRefresh;
const refreshReady = new Promise(resolve => { releaseRefresh = resolve; });
const check = (actual, expected) => { assert.deepEqual(actual, expected); cases++; };
function sshSetupResponse(body) {
  setupRequests.push(body);
  if (body.action === "status") return { state: "form", saved: setupDeclined ? [declinedResult()] : [] };
  if (body.action === "decline" && body.previewId === `preview-${previewCount}`) {
    setupDeclined = true;
    return declinedResult();
  }
  if (body.action === "apply" && body.previewId === `preview-${previewCount}`) {
    setupDeclined = false;
    return { state: "applied", message: "已交付 Codex 打开请求。请核对主机和目录，再发送第一句话。" };
  }
  if ((body.action === "preview" && !setupDeclined) ||
      (body.action === "restore" && setupDeclined && body.targetId === "isolated-target")) {
    return { state: "preview", previewId: `preview-${++previewCount}`, summary: setupSummary, canApply: true,
      changes: [
        { label: "新增 ~/.ssh/codex-session-board.conf",
          detail: "+ Host csb-studio\n+     HostName studio.example.test\n+     User builder\n+     Port 22" },
        { label: "在 ~/.ssh/config 顶层加入 Include", detail: "+ Include ~/.ssh/codex-session-board.conf" },
        { label: "新增 Codex 连接、登记 /srv/project 并开启该连接的自动连接" },
      ] };
  }
  return { error: "此操作不可用" };
}
async function openSshSettings(page) {
  await until(
    () => page.evaluate('document.getElementById("ssh-settings")?.hidden === false && !filtersLocked()'),
    "连接设置按钮未就绪"
  );
  await clickNode(page, "#ssh-settings");
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
    const payload = JSON.parse(body);
    if (request.url === "/api/new" && requireConsent && !payload.consent) {
      consentPreviews.push(payload);
      return response.writeHead(200, { "Content-Type": "application/json" })
        .end(JSON.stringify({ consent: { token: "approved-connection", summary: "Mac · /remote/project",
          changes: ["开启该连接的自动连接"] } }));
    }
    writes.push({ path: request.url, body: payload });
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
  await evaluate('byId("host-filter").value="all";byId("host-filter").dispatchEvent(new Event("change"))');
  await clickNode(page, "#new-session");
  check(await evaluate('Array.from(byId("new-host").options).map(option=>option.textContent)'), ["本机","Mac · remote:a","Mac · remote:b"]);
  for (const [host, expected] of [["remote:a", project], ["remote:b", otherProject]]) {
    await evaluate(`byId("new-host").value=${JSON.stringify("host:" + host)};
      byId("new-host").dispatchEvent(new Event("change"))`);
    check(await evaluate('Array.from(byId("new-project").options).map(option=>option.value)'), [expected]);
  }
  await clickNode(page, "#new-target-cancel");
  await wait('!byId("new-target").open && !byId("host-filter").disabled');
  check(writes, []);
  await evaluate('byId("host-filter").value="host:remote:a";byId("host-filter").dispatchEvent(new Event("change"))');
  await evaluate(`byId("project-filter").value=${JSON.stringify("project:" + project)};
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
  await clickNode(page, "#assignment-cancel");
  await wait('!byId("project-assignment").open && !byId("host-filter").disabled');
  check(writes.length, 1);
  await clickNode(page, "[data-action=assign-project]");
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

  requireConsent = true;
  await evaluate(`createSession(${JSON.stringify(project)})`);
  check(await evaluate('byId("native-consent-changes").textContent'), "开启该连接的自动连接");
  const beforeCancel = writes.length;
  await clickNode(page, "#native-consent-cancel");
  check(writes.length, beforeCancel);
  await evaluate(`createSession(${JSON.stringify(project)})`);
  await evaluate('byId("native-consent").dispatchEvent(new Event("close"))');
  await evaluate('byId("native-consent-form").requestSubmit()');
  await wait('!creating && !byId("native-consent").open');
  check(consentPreviews.length, 2);
  check(consentPreviews[0], consentPreviews[1]);
  check(writes.at(-1).body, { ...consentPreviews[0], consent: "approved-connection" });
  requireConsent = false;

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
  await evaluate("void load()");
  await openSshSettings(page);
  await wait('sshSetup?.state==="form" && !sshSetupBusy');
  await evaluate(`for (const [name,value] of Object.entries(${JSON.stringify(setupTarget)}))
    byId("ssh-setup-form").elements.namedItem(name).value=value`);
  await clickNode(page, "#ssh-setup-next");
  await wait('sshSetup?.state==="preview" && !sshSetupBusy');
  check(setupRequests.at(-1), { action: "preview", target: setupTarget });
  check(await evaluate('byId("ssh-setup-next").disabled'), false);
  check(await evaluate('byId("ssh-setup-changes").children.length'), 3);
  await capture("02-authorization-preview");
  await clickNode(page, "#ssh-setup-decline");
  await wait('sshSetup?.state==="declined" && !sshSetupBusy');
  check(setupRequests.at(-1), { action: "decline", previewId: "preview-1" });
  const reopened = await browser.page(address);
  await openSshSettings(reopened);
  await until(() => reopened.evaluate('sshSetup?.saved?.[0]?.state==="declined" && !sshSetupBusy'), "拒绝状态未读回");
  check(await reopened.evaluate('byId("ssh-setup-saved").hidden'), false);
  if (evidence) {
    const image = await reopened.command("Page.captureScreenshot", { format: "png" });
    await fs.writeFile(`${evidence}/02-declined.png`, Buffer.from(image.data, "base64"));
  }
  await clickNode(reopened, "#ssh-setup-saved button");
  await until(() => reopened.evaluate('sshSetup?.state==="preview" && !sshSetupBusy'), "重新启用预览未读回");
  check(setupRequests.at(-1), { action: "restore", targetId: "isolated-target" });
  check(await reopened.evaluate('sshSetup.previewId'), "preview-2");
  await clickNode(reopened, "#ssh-setup-next");
  await until(() => reopened.evaluate('sshSetup?.state==="applied" && !sshSetupBusy'), "配置回执未读回");
  check(setupRequests.at(-1), { action: "apply", previewId: "preview-2" });
  check(writes.length, checkpoints);
  check(JSON.stringify(fixture.health.remoteRegistry), registration);
} finally {
  releaseAssignment();
  releaseRefresh();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({ passed: true, cases, cleanup }));
