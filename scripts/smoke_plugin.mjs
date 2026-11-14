import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { launchBrowser } from "./browser-client.mjs";
import { until, clickNode, dragCard, inputText } from "./browser-actions.mjs";
import { fields, unchanged, stateWrite } from "./smoke-guards.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({ options: {
  receipt: { type: "string" }, "session-id": { type: "string" }, output: { type: "string" },
} });
assert(values.receipt && values.output && /^[0-9a-f-]{36}$/.test(values["session-id"] || ""),
  "需要 --receipt、--session-id 和 --output");
const output = path.resolve(values.output), started = performance.now();
assert(output !== root && !output.startsWith(root + path.sep), "报告必须放在源码之外");
await fs.mkdir(output, { recursive: false });
const report = { passed: false, surface: "live_http_page", native_container_verified: false,
  id: values["session-id"], started_at: new Date().toISOString(), cases: [], restored: false };
let proof, browser, page, original, owned, baseline, paused;
const evaluate = expression => page.evaluate(expression);
const read = async pathname => {
  const response = await fetch(new URL(pathname, proof.url), { redirect: "error", signal: AbortSignal.timeout(15000) });
  assert(response.ok, `HTTP ${response.status}`);
  return response.json();
};
const current = async () => (await read("/api/sessions")).sessions.find(row => row.id === report.id);
const verify = filename => {
  execFileSync(path.join(root, "scripts/verify-plugin"), [
    "--state", proof.installation_state,
    "--config", proof.config, "--client-pid", String(proof.client_pid),
    "--expected-source", proof.source_commit, "--config-id", proof.runtime.configId,
    "--output", path.join(output, filename),
  ], { encoding: "utf8", timeout: 30000, stdio: ["ignore", "pipe", "pipe"] });
};
const check = async (name, operation) => {
  const start = performance.now();
  await operation();
  report.cases.push({ name, passed: true, seconds: (performance.now() - start) / 1000 });
};
const select = (selector, value) => evaluate(`(()=>{
  const node=document.querySelector(${JSON.stringify(selector)});
  node.value=${JSON.stringify(value)};node.dispatchEvent(new Event("change",{bubbles:true}));
})()`);
const identity = async () => assert.deepEqual(await read("/api/info"), proof.runtime, "服务身份已变化");
const checkpoint = async submitted => {
  const temporary = path.join(output, "recovery.tmp");
  await fs.writeFile(temporary, JSON.stringify({
    source_commit: proof.source_commit, before: fields(original), owned: fields(owned),
    submitted, restored: report.restored,
  }, null, 2) + "\n");
  await fs.rename(temporary, path.join(output, "RECOVERY.json"));
};
const permit = async () => {
  await identity();
  unchanged(await current(), owned);
  const body = stateWrite(paused.request, owned, proof.url);
  await checkpoint(body);
  await page.command("Fetch.continueRequest", { requestId: paused.requestId });
  paused = null;
  return body;
};
const receive = async operation => {
  const incoming = page.event("Fetch.requestPaused");
  incoming.catch(() => {});
  await operation();
  paused = await incoming;
};
const acknowledged = async body => {
  await until(() => evaluate("!saving"), "保存没有结束");
  const row = await current();
  assert.equal(row.version, owned.version + 1, "保存版本不是本次操作");
  assert.equal(row.inputVersion, owned.inputVersion);
  assert.equal(row.sectionVersion, owned.sectionVersion);
  assert.equal(row.note, body.note.trim());
  assert.equal(row.manual, body.state !== null);
  if (body.state !== null) assert.equal(row.state, body.state);
  owned = structuredClone(row);
  await checkpoint(null);
  await evaluate("load()");
};
const open = async () => {
  await dragCard(page, report.id, "blocked");
  await until(() => evaluate('document.querySelector("#editor").open'), "阻塞弹窗未打开");
};
const saveNote = async text => {
  await open();
  await inputText(page, "#note", text);
  await receive(() => clickNode(page, "#save"));
  const body = await permit();
  await acknowledged(body);
};
const screenshot = async name => {
  const image = await page.command("Page.captureScreenshot", { format: "png" });
  await fs.writeFile(path.join(output, name), Buffer.from(image.data, "base64"));
};
try {
  proof = JSON.parse(await fs.readFile(values.receipt, "utf8"));
  assert(proof.passed && proof.phase === "desktop_loaded", "需要当前客户端的成功核验回执");
  assert(/^http:\/\/127\.0\.0\.1:\d+$/.test(proof.url), "只允许已核验的回环服务");
  assert(proof.installation_state, "核验回执缺少安装状态目录");
  verify("runtime-before.json");
  assert.equal(createHash("sha256").update(await fs.readFile(proof.config)).digest("hex"), proof.config_sha256);
  const config = JSON.parse(await fs.readFile(proof.config, "utf8"));
  assert(config.sync_titles === false && config.sync_sections === false && config.native_sections !== true
    && config.auto_archive_seconds == null && config.remote_auto_archive_seconds == null,
  "写入冒烟要求关闭标题、分区同步及自动归档，其他模式请使用隔离环境");
  await identity();
  baseline = await read("/api/sessions");
  original = baseline.sessions.find(row => row.id === report.id);
  assert(original && !original.remote && !original.readOnly, "必须明确指定可写的本机测试卡片");
  owned = structuredClone(original);
  await checkpoint(null);
  report.before = fields(original);
  report.source_commit = proof.source_commit;
  report.binary_sha256 = proof.binary_sha256;
  browser = await launchBrowser();
  page = await browser.page(proof.url);
  await until(() => evaluate('typeof data!=="undefined" && data?.health.runtimeVerified && data?.health.scanComplete && !data?.health.error && !data?.health.runtimeError && !data?.health.nativeError && data?.projectCatalog.loaded'),
    "实际看板尚未就绪");
  await page.command("Fetch.enable", { patterns: [{ urlPattern: new URL("/api/state", proof.url).href, requestStage: "Request" }] });
  await check("主机、项目和搜索筛选", async () => {
    await select("#host-filter", "local");
    const project = original.projectId || baseline.projectCatalog.items.find(item => !item.remote)?.id;
    assert(project, "缺少可验证的本机项目");
    await select("#project-filter", "project:" + project);
    const expected = baseline.sessions.filter(row => !row.remote && row.projectId === project).map(row => row.id).sort();
    assert.deepEqual(await evaluate('[...document.querySelectorAll(".card")].map(node=>node.dataset.id).sort()'), expected);
    report.project_count = expected.length;
    await select("#project-filter", "all");
    await clickNode(page, "#filter-toggle");
    await inputText(page, "#search", original.title);
    assert(await evaluate(`Boolean(document.querySelector('.card[data-id="${report.id}"]'))`));
    await inputText(page, "#search", "");
    await clickNode(page, "#filter-toggle");
  });
  await check("取消不修改状态、备注或版本", async () => {
    await open();
    await inputText(page, "#note", "冒烟测试，不应保存");
    await clickNode(page, "#cancel");
    await until(() => evaluate('!document.querySelector("#editor").open'), "取消没有关闭弹窗");
    unchanged(await current(), owned);
  });
  await check("空白原因保存为空且人工确认", () => saveNote(" \t\n　"));
  await check("非空备注及显式清空", async () => {
    await saveNote("CLI现场冒烟测试");
    assert.equal(owned.note, "CLI现场冒烟测试");
    await saveNote("");
  });
  await check("真实等待提示、临时落位与服务端确认", async () => {
    await receive(() => dragCard(page, report.id, "review"));
    unchanged(await current(), owned);
    assert(await evaluate('Boolean(document.querySelector(".move-pending")) && saving'));
    await until(() => evaluate('!document.querySelector("#move-feedback").hidden'), "未出现等待提示");
    report.early_feedback_ms = await evaluate("performance.now()-moving.started");
    await select("#host-filter", "all");
    assert.equal(await evaluate('document.querySelector("#host-filter").disabled'), false);
    await select("#host-filter", "local");
    await until(() => evaluate("moving?.slow === true"), "未出现慢响应提示");
    report.slow_feedback_ms = await evaluate("performance.now()-moving.started");
    unchanged(await current(), owned);
    await screenshot("pending.png");
    const body = await permit();
    await acknowledged(body);
    assert.equal(owned.state, "review");
  });
} catch (error) {
  report.error = error.stack;
  if (page) {
    try {
      report.diagnostics = await evaluate('({health:typeof data==="undefined"?null:data?.health,notice:document.querySelector("#notice")?.textContent,feedback:document.querySelector("#feedback")?.textContent})');
    } catch (diagnosticError) { report.diagnostic_error = diagnosticError.message; }
  }
} finally {
  try {
    if (paused && page) {
      await page.command("Fetch.failRequest", { requestId: paused.requestId, errorReason: "Aborted" });
      paused = null;
      await until(() => evaluate("!saving"), "取消测试请求后页面仍在保存");
    }
    if (page && original && owned) {
      await identity();
      unchanged(await current(), owned);
      if (owned.version !== original.version) {
        const incoming = page.event("Fetch.requestPaused");
        incoming.catch(() => {});
        const restored = evaluate(`post("/api/state",${JSON.stringify({
          id: original.id, state: original.manual ? original.state : null, note: original.note,
          version: owned.version, inputVersion: owned.inputVersion, sectionVersion: owned.sectionVersion,
        })})`);
        restored.catch(() => {});
        paused = await incoming;
        const body = await permit();
        await restored;
        await acknowledged(body);
      }
      assert.equal(owned.note, original.note);
      assert.equal(owned.manual, original.manual);
      if (original.manual) assert.equal(owned.state, original.state);
      report.restored = true;
      await checkpoint(null);
      report.after = fields(owned);
      const after = await read("/api/sessions");
      assert(baseline.sessions.every(before => after.sessions.some(row => row.id === before.id && row.note === before.note)),
        "原有会话或备注发生变化");
      report.original_notes_preserved = baseline.sessions.length;
      await select("#host-filter", "all");
      await screenshot("restored.png");
      verify("runtime-after.json");
      assert.equal(createHash("sha256").update(await fs.readFile(proof.config)).digest("hex"), proof.config_sha256);
    }
  } catch (error) { report.recovery_error = error.stack; }
  try { if (browser) report.cleanup = await browser.close(); }
  catch (error) { report.cleanup_error = error.stack; }
  report.seconds = (performance.now() - started) / 1000;
  report.passed = !report.error && !report.recovery_error && !report.cleanup_error && report.restored && report.cases.length === 5;
  await fs.writeFile(path.join(output, "SMOKE.json"), JSON.stringify(report, null, 2) + "\n");
}
console.log(JSON.stringify({ passed: report.passed, seconds: report.seconds, cases: report.cases.length,
  restored: report.restored, error: report.error, recovery_error: report.recovery_error, output }));
process.exitCode = report.passed ? 0 : 1;
