import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import { mkdtemp, mkdir, readdir, realpath, rm } from "node:fs/promises";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { randomUUID, createHash } from "node:crypto";
import vm from "node:vm";

const { candidate } = JSON.parse(readFileSync(0, "utf8"));
assert(process.argv[2] && path.isAbsolute(process.argv[2]), "Pass an absolute native backend executable");
const main = Object.keys(candidate).find(name => name.includes("/early-bootstrap-226-"));
assert(main, "The generated candidate must contain the native creation module");
function extract(marker, context, source = candidate[main]) {
  const start = source.indexOf(marker);
  assert(start >= 0, marker);
  let end = start;
  while ((end = source.indexOf("}", end + 1)) >= 0) {
    let script;
    try { script = new vm.Script(`(${source.slice(start, end + 1)})`); } catch { continue; }
    return script.runInNewContext(context);
  }
  throw Error(`Incomplete native boundary ${marker}`);
}
const Creation = extract("class xz{", {
  nf: { iw: "cloud" }, xv: async tier => tier, xj: () => null,
  xP: value => value, xC: () => {}, xL: async request => ({ config: request.config }),
  bz: "fixture-service",
});
const normalize = extract("async function _D(", {
  nf: { iw: "cloud" }, SV: { zO: () => null },
});
const runtimeSource = Object.values(candidate).find(source => source.includes("function targetDraftRuntime("));
assert(runtimeSource, "The generated candidate must contain its target runtime");
const targetRuntimeFactory = extract("function targetDraftRuntime(", { setTimeout }, runtimeSource);
const root = await realpath(await mkdtemp(path.join(os.tmpdir(), "native-target-start-")));
const home = path.join(root, "home");
await mkdir(home);
const requests = [];
const events = [];
const pending = new Map();
let backend, serial = 0, buffer = "", diagnostic = "";
const server = http.createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  requests.push({ method: request.method, path: request.url });
  assert.equal(request.method, "POST");
  assert(request.url.endsWith("/responses"));
  JSON.parse(body);
  const item = { id: "msg_fixture", type: "message", role: "assistant", status: "completed",
    content: [{ type: "output_text", text: "ISOLATED_FIXTURE_OK", annotations: [] }] };
  response.writeHead(200, { "Content-Type": "text/event-stream", Connection: "close" });
  for (const event of [
    { type: "response.created", response: { id: "resp_fixture", status: "in_progress" } },
    { type: "response.output_item.added", output_index: 0, item: { ...item, content: [] } },
    { type: "response.output_item.done", output_index: 0, item },
    { type: "response.completed", response: { id: "resp_fixture", status: "completed",
      output: [item], usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2,
        input_tokens_details: { cached_tokens: 0 } } } },
  ]) response.write(`data: ${JSON.stringify(event)}\n\n`);
  response.end();
});
const call = (method, params) => new Promise((resolve, reject) => {
  const id = ++serial;
  const timer = setTimeout(() => {
    pending.delete(id); reject(Error(`${method} timed out: ${diagnostic}`));
  }, 15000);
  pending.set(id, { resolve, reject, timer });
  backend.stdin.write(JSON.stringify({ id, method, params }) + "\n");
});
const runtime = targetRuntimeFactory({
  createId: () => "client-new-thread:fixture", scopeId: scope => scope.id,
  existing: () => undefined, readContext: () => ({ accountId: "account", userId: "user" }),
  request: (scope, hostId, method, params) => {
    assert.equal(hostId, "remote:isolated");
    return call(method, params);
  },
});
async function completed(threadId) {
  const deadline = Date.now() + 15000;
  while (!events.some(event => event.method === "turn/completed" && event.params?.threadId === threadId)) {
    assert(Date.now() < deadline, `First turn did not complete: ${diagnostic}`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}
async function create(project, cwd) {
  const target = runtime.register({
    hostId: "remote:isolated", projectId: project.id, operationId: randomUUID(),
  });
  await runtime.resolve({}, target);
  const normalized = await normalize({
    getHostId: () => target.hostId, readDefaultPersonality: async () => null,
  }, { readDefaultFeatureOverrides: () => null }, {
    cwd, workspaceRoots: [cwd], workspaceKind: "project", nativeTargetDraft: target,
    permissionsConfig: { sandboxPolicy: { type: "workspaceWrite", writableRoots: [cwd] } },
  });
  assert.equal(normalized.nativeTargetDraft.projectId, project.id);
  assert.equal(normalized.projectAssignment, undefined);
  const creation = new Creation({ hostId: target.hostId, logger: {}, requestClient: {},
    inputs: { hasDesktopRuntime: true, isBrowserRuntime: false,
      projectId: normalized.nativeTargetDraft.projectId } });
  creation.buildNewConversationParams = async (model, tier, selectedCwd) => ({
    cwd: selectedCwd, model: "isolated-fixture-model", approvalPolicy: "never", config: {},
    sandboxPolicy: { type: "workspaceWrite", writableRoots: [cwd] }, ephemeral: false,
  });
  const prepared = await creation.prepareConversation(normalized, "paginated");
  assert.equal(prepared.canUsePrewarmedThread, false);
  const start = await prepared.prepareStart();
  assert.equal(start.request.projectId, project.id);
  assert.equal(start.request.cwd, cwd);
  const { thread } = await call(start.method, JSON.parse(JSON.stringify(start.request)));
  runtime.accepted(target, thread.id);
  await runtime.beforeFirstTurn({}, target);
  await call("turn/start", { threadId: thread.id, cwd,
    input: [{ type: "text", text: "Return the local fixture marker" }] });
  await runtime.verify({}, target, thread.id);
  await completed(thread.id);
  const current = (await call("thread/read", { threadId: thread.id, includeTurns: false })).thread;
  assert.equal(current.projectId, project.id);
  assert.equal(current.cwd, cwd);
  return { threadId: thread.id, projectId: project.id, cwd, prewarm: false };
}
try {
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${server.address().port}/v1`;
  backend = spawn(process.argv[2], [
    "-c", 'model_provider="isolated_fixture"', "-c", 'model="isolated-fixture-model"',
    "-c", 'model_providers.isolated_fixture.name="Isolated local fixture"',
    "-c", `model_providers.isolated_fixture.base_url=${JSON.stringify(base)}`,
    "-c", 'model_providers.isolated_fixture.wire_api="responses"',
    "-c", "model_providers.isolated_fixture.requires_openai_auth=false",
    "app-server", "--listen", "stdio://",
  ], { cwd: root, env: { PATH: process.env.PATH, HOME: root, CODEX_HOME: home, TMPDIR: root },
    stdio: ["pipe", "pipe", "pipe"] });
  backend.stderr.on("data", chunk => { diagnostic = (diagnostic + chunk).slice(-2000); });
  backend.on("error", error => {
    for (const waiter of pending.values()) { clearTimeout(waiter.timer); waiter.reject(error); }
    pending.clear();
  });
  backend.stdout.on("data", chunk => {
    buffer += chunk;
    while (buffer.includes("\n")) {
      const index = buffer.indexOf("\n"), line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (!line) continue;
      const message = JSON.parse(line);
      if (!Object.hasOwn(message, "id")) { events.push(message); continue; }
      const waiter = pending.get(message.id);
      if (!waiter) continue;
      pending.delete(message.id); clearTimeout(waiter.timer);
      if (message.error) waiter.reject(Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    }
  });
  await call("initialize", { clientInfo: { name: "codex_desktop", version: "isolated-fixture" },
    capabilities: { experimentalApi: true } });
  backend.stdin.write('{"method":"initialized"}\n');
  const cwd = path.join(root, "same-workspace");
  await mkdir(cwd);
  const results = [];
  for (const label of ["A", "B"]) {
    const { project } = await call("project/create", { idempotencyKey: randomUUID(),
      name: `Isolated target ${label}`, roots: [{ path: cwd }], metadata: {} });
    results.push(await create(project, cwd));
  }
  const database = (await readdir(home)).find(name => /^state_.*\.sqlite$/.test(name));
  assert(database, "Native backend did not materialize its state database");
  const rows = JSON.parse(execFileSync("python3", ["-B", "-c",
    'import json,sqlite3,sys;db=sqlite3.connect("file:"+sys.argv[1]+"?mode=ro",uri=True);print(json.dumps(db.execute("select id,project_id,cwd from threads").fetchall()))',
    path.join(home, database)], { encoding: "utf8", timeout: 10000 }));
  assert(results.every(result => rows.some(row => row[0] === result.threadId &&
    row[1] === result.projectId && row[2] === result.cwd)));
  console.log(JSON.stringify({ passed: true, localModelRequests: requests.length,
    externalModelRequests: 0, sqliteMatches: true, acceptedReadbackVerified: true, projects: results,
    candidateModule: createHash("sha256").update(candidate[main]).digest("hex") }));
} finally {
  for (const waiter of pending.values()) { clearTimeout(waiter.timer); waiter.reject(Error("Fixture closed")); }
  pending.clear();
  if (backend?.pid && backend.exitCode === null) {
    backend.kill("SIGTERM");
    await once(backend, "close");
  }
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(root, { recursive: true, force: true });
}
