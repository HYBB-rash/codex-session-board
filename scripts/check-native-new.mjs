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
  health: { remoteRegistry: [{ hostId: "remote:a", hostLabel: "Mac", enabled: true }] },
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
let failAssignment = true, browser, cleanup, cases = 0;
let releaseAssignment;
const assignmentReady = new Promise(resolve => { releaseAssignment = resolve; });
const check = (actual, expected) => { assert.deepEqual(actual, expected); cases++; };
const server = http.createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.method === "POST") {
    writes.push({ path: request.url, body: JSON.parse(body) });
    if (request.url === "/api/project/assign") await assignmentReady;
    if (request.url === "/api/project/assign" && failAssignment)
      return response.writeHead(409, { "Content-Type": "application/json" })
        .end(JSON.stringify({ error: "补登记未确认，请对同一会话重试" }));
    if (request.url === "/api/project/assign") fixture.sessions[0].projectId = project;
  }
  response.writeHead(200, { "Content-Type": request.url === "/" ? "text/html; charset=utf-8" : "application/json" });
  response.end(request.url === "/" ? html : JSON.stringify(request.method === "POST" ? { ok: true } : fixture));
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
try {
  browser = await launchBrowser();
  const page = await browser.page(`http://127.0.0.1:${server.address().port}/`);
  const evaluate = expression => page.evaluate(expression);
  const wait = expression => until(() => evaluate(expression), "页面状态未就绪：" + expression);
  await wait('typeof data !== "undefined" && data.sessions.length === 1');
  await evaluate('byId("host-filter").value="host:remote:a";byId("host-filter").dispatchEvent(new Event("change"))');
  check(await evaluate('canCreateProject(data.projectCatalog.items[0])'), true);
  await evaluate('createSession(data.projectCatalog.items[0].id)');
  await wait("!creating");
  check(writes, [{ path: "/api/new", body: { projectId: project } }]);
  check(await evaluate('byId("feedback").textContent.includes("发送第一句话后才会创建会话")'), true);
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
  check(writes.filter(write => write.path === "/api/project/assign").map(write => write.body),
    [{ id: identifier, projectId: project, confirmed: true }, { id: identifier, projectId: project, confirmed: true }]);
  check(writes.filter(write => write.path === "/api/new").length, 1);
  check(await evaluate('data.sessions.length'), 1);
  check(await evaluate('document.querySelector("[data-action=assign-project]") === null'), true);
} finally {
  releaseAssignment();
  if (browser) cleanup = await browser.close();
  await new Promise(resolve => server.close(resolve));
}
console.log(JSON.stringify({ passed: true, cases, cleanup }));
