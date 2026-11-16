import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

process.setUncaughtExceptionCaptureCallback(error => {
  console.error(error.stack);
  process.exitCode = 1;
});

const { original, candidate } = JSON.parse(readFileSync(0, "utf8"));
const plain = value => JSON.parse(JSON.stringify(value));
function noop() {}
const fallback = new Proxy(noop, {
  get: (target, key) => key === "then" ? undefined : fallback,
  apply: () => fallback, construct: () => ({}),
});
const atom = (scope, initial) => ({ initial });
const derived = (scope, read) => ({ read });
function loader(sources, overrides) {
  const modules = {};
  const cache = new Map();
  const allowed = new Set(["V3rO", "HSUP", "QX7Q", "TSah", "e1yv", "aVy5"]);
  for (const [path, source] of Object.entries(sources)) {
    if (!path.includes("/webview/")) continue;
    const context = {};
    vm.runInNewContext(source.replaceAll("export const ", "var "), context);
    Object.assign(modules, context.__webpack_modules__);
  }
  const require = id => {
    if (id in overrides) return overrides[id];
    if (cache.has(id)) return cache.get(id);
    if (!allowed.has(id) || !modules[id]) return fallback;
    const exports = {};
    cache.set(id, exports);
    modules[id]({}, exports, require);
    return exports;
  };
  require.d = (exports, getters, values) => {
    Object.assign(exports, values);
    for (const [name, get] of Object.entries(getters))
      Object.defineProperty(exports, name, { get, configurable: true });
  };
  require.n = value => () => value;
  return require;
}
const symbols = new Proxy({}, { get: (target, key) => target[key] ??= Symbol(key) });
const projects = new Map(["project-a", "project-b"].map(id => [
  id, { id, name: id, roots: [{ path: "/same" }, { path: "/second" }] },
]));
const requests = [];
const pendingDeletions = {};
const overrides = {
  zTeN: { z8: name => ({ id: name }), vP: atom, i1: derived, bD: derived,
    un: derived, T1: derived, vU: derived, ZY: noop },
  ASxG: fallback,
  Nwp9: { m3: (name, initial) => ({ initial }),
    Kn: (name, initial) => ({ initial: name === "pending-deletions" ? pendingDeletions : initial }) },
  DBCl: new Proxy({
    xa2: () => "client-new-thread:random", L$F: id => id.startsWith("client-new-thread:"),
    Z3o: value => value, xwm: () => false, _RT: id => `/threads/${id}`,
    Q2Y: { APP_SERVER_PENDING_PROJECT_DELETIONS_BY_HOST: "pending-deletions" },
    JFB: () => true,
  }, { get: (target, key) => key in target ? target[key] : fallback }),
  "gZd7": { H: { initial: null }, b: noop },
  OWtl: { u: { id: "ShellScope" } },
  Dgwo: { V: (scope, host) => ({
    sendRequest: async (method, params) => {
      requests.push({ host, method, params });
      assert.equal(host, "remote:target");
      if (method === "config/read") return { config: { model_provider: "fixture" } };
      assert.equal(method, "project/read");
      return { project: structuredClone(projects.get(params.projectId)) };
    },
  }) },
  RAsy: { Y: value => value },
  SPnB: { chatGptAuthenticatedAccountId$: { initial: "account" },
    chatGptAuthenticatedUserId$: { initial: "user" }, PO: { initial: { authLoading: false } } },
  m8L3: { Y: null },
  HVBu: handler => handler,
  gfHD: { V3: () => ({}) },
  wCOm: { Y$: value => value },
  r0y7: { tu: async () => null },
  xKli: { NS: () => [], Fc: (context, prompt) => ({ input: { type: "text", text: prompt } }),
    gx: noop, kv: context => context.prompt },
  LnAF: { v: value => value },
  ZjPZ: { E5: value => value },
  Vrnh: { jA: value => value },
  Sd8U: { SN: () => "new-conversation" },
  CDw0: { FA: () => ({ prepare: async value => value, settle: noop, settleError: noop }) },
  HK0a: { jz: noop },
  IqGC: { o4: () => async () => false },
  NF58: { wW: () => false },
  seW7: { jo: value => ({ ...value, createdAt: 1 }) },
  TAOo: { K: value => value },
  OCQL: { pM: () => noop },
};
function extractFunction(source, marker) {
  const start = source.indexOf(marker);
  assert(start >= 0, marker);
  let end = start;
  while ((end = source.indexOf("}", end + 1)) >= 0) {
    try { return new vm.Script(`(${source.slice(start, end + 1)})`); } catch {}
  }
  throw Error(`Incomplete native function ${marker}`);
}
const mainPath = Object.keys(candidate).find(path => path.includes("/early-bootstrap-226-"));
const functionContext = {
  nf: { iw: "cloud" },
  SV: { zO: () => null },
};
const controlNormalize = extractFunction(original[mainPath], "async function _D(")
  .runInNewContext(functionContext);
const normalize = extractFunction(candidate[mainPath], "async function _D(")
  .runInNewContext(functionContext);
const Builder = extractFunction(candidate[mainPath], "class xz").runInNewContext({
  ...functionContext, xv: async tier => tier, xj: () => null, xP: value => value,
  xC: noop, xL: async request => ({ config: request.config }), Cz: () => true,
  bz: "native-default",
});
async function startRequest(hostId, inputs, params) {
  const builder = new Builder({ hostId, inputs, logger: {}, requestClient: {} });
  builder.buildNewConversationParams = async (model, tier, cwd) => ({ cwd, config: {} });
  const prepared = await builder.prepareConversation(params);
  const start = await prepared.prepareStart();
  return { prepared, start };
}
const require = loader(candidate, overrides);
const native = require("V3rO");
function scope(clientThreadId) {
  const state = new Map();
  const current = { value: { kind: "new", entrypoint: "home", clientThreadId },
    chain: new Map([["ThreadScope", { value: { clientThreadId } }]]),
    get: (key, argument) => {
      if (state.has(key)) return state.get(key);
      if (key?.read) return key.read(argument, { get: current.get, scope: current });
      return key?.initial;
    },
    set: (key, value) => state.set(key, typeof value === "function" ? value(current.get(key)) : value),
  };
  return current;
}
const targets = ["project-a", "project-b"].map((projectId, index) => native.registerNativeTargetDraft({
  hostId: "remote:target", projectId,
  operationId: `019a1111-2222-4333-8444-${index ? "666666666666" : "555555555555"}`,
}));
assert.notEqual(targets[0].clientThreadId, targets[1].clientThreadId);
const controls = loader(original, overrides);
const expectedParams = [];
const submitParams = [];
for (const target of targets) {
  const current = scope(target.clientThreadId);
  const workspace = await native.resolveNativeTargetDraft(current, target);
  current.set(native.nativeTargetWorkspace, { status: "ready", workspace });
  const view = require("HSUP").G.read({}, { get: current.get, scope: current });
  assert.equal(view.target.hostId, target.hostId);
  assert.equal(view.resolvedCwd, "/same");
  assert.deepEqual(plain(view.remoteWorkspaceRoots), ["/same", "/second"]);
  const membership = require("HSUP").s.read({}, { get: current.get, scope: current });
  assert.equal(membership.threadProjectAssignment, undefined);
  const initial = await require("e1yv").l({
    scope: current, context: { fileAttachments: [], addedFiles: [] }, prompt: "fixture",
    ...workspace, hostId: target.hostId, threadCreationHostId: target.hostId,
    shouldSendPermissionOverrides: false, nativeTargetDraft: plain(target),
  });
  const control = controls("QX7Q").k(initial);
  assert.equal(control.nativeTargetDraft, undefined);
  const converted = require("QX7Q").k(initial);
  const manager = { getHostId: () => target.hostId, readDefaultPersonality: async () => null };
  const settings = { readDefaultFeatureOverrides: () => ({}) };
  const lost = await controlNormalize(manager, settings, converted, {});
  assert.equal(lost.nativeTargetDraft, undefined);
  const normalized = await normalize(manager, settings, converted, {});
  assert.equal(normalized.nativeTargetDraft.projectId, target.projectId);
  assert.equal(normalized.projectAssignment, undefined);
  const reader = require("TSah").v(current, target.hostId);
  reader.readInputs = async () => ({ hasDesktopRuntime: true });
  const inputs = await reader.readCreationInputs(normalized);
  assert.equal(inputs.projectId, target.projectId);
  const { prepared, start } = await startRequest(target.hostId, inputs, normalized);
  assert.equal(prepared.canUsePrewarmedThread, false);
  assert.equal(start.method, "thread/start");
  assert.equal(start.request.projectId, target.projectId);
  assert.equal(start.request.cwd, "/same");
  assert.deepEqual(plain(start.request.runtimeWorkspaceRoots), ["/same", "/second"]);
  const controlStart = await startRequest(target.hostId, { hasDesktopRuntime: true }, lost);
  assert.equal(controlStart.start.request.projectId, undefined);
  const warmParams = { ...normalized, permissionsConfig: { sandboxPolicy: { type: "workspaceWrite" } } };
  const controlWarm = await startRequest(target.hostId, { hasDesktopRuntime: true }, warmParams);
  const targetWarm = await startRequest(target.hostId, inputs, warmParams);
  assert.equal(controlWarm.prepared.canUsePrewarmedThread, true);
  assert.equal(targetWarm.prepared.canUsePrewarmedThread, false);
  expectedParams.push({ projectId: inputs.projectId, cwd: normalized.cwd,
    roots: normalized.workspaceRoots, assignment: normalized.projectAssignment });
  let submitted;
  const actions = require("aVy5").N({
    scope: current, clientThreadId: target.clientThreadId, resolvedHostId: "operator-host",
    newConversationNavigation: "none", shouldSendPermissionOverrides: false,
    isMounted: () => true,
    startConversationWithPrimaryRuntimeForFirstTurn: async params => {
      submitted = params;
      throw Error("Stopped before native dispatch");
    },
  });
  try {
    await actions.handleSubmitLocal({ prompt: "fixture", localProjectId: "operator-project",
      remoteProjectId: "operator-remote", fileAttachments: [], addedFiles: [],
      imageAttachments: [], pastedTextAttachments: [], workspaceRoots: ["/operator"] }, "/operator");
  } catch (error) {
    if (!submitted) throw error;
  }
  assert(submitted);
  assert.equal(submitted.hostId, target.hostId);
  assert.equal(submitted.clientThreadId, target.clientThreadId);
  assert.equal(submitted.baseParams.nativeTargetDraft.projectId, target.projectId);
  assert.equal(submitted.baseParams.projectAssignment, undefined);
  assert.equal(submitted.baseParams.cwd, "/same");
  assert.deepEqual(plain(submitted.baseParams.workspaceRoots), ["/same", "/second"]);
  const submittedNormalized = await normalize(manager, settings, submitted.baseParams, {});
  const submittedInputs = await reader.readCreationInputs(submittedNormalized);
  const submittedStart = await startRequest(target.hostId, submittedInputs, submittedNormalized);
  assert.equal(submittedStart.start.request.projectId, target.projectId);
  assert.equal(submittedStart.start.request.cwd, "/same");
  assert.deepEqual(plain(submittedStart.start.request.runtimeWorkspaceRoots), ["/same", "/second"]);
  submitParams.push(submitted);
}
assert.deepEqual(expectedParams.map(plain), targets.map(target => ({
  projectId: target.projectId, cwd: "/same", roots: ["/same", "/second"],
})));
assert(requests.length >= 4);
assert.equal(submitParams.length, 2);
const submitPath = Object.keys(candidate).find(path => path.includes("/486887."));
const submitSource = candidate[submitPath];
const prewarmStart = submitSource.indexOf("}({activeCollaborationMode:ix,agentMode:fu");
assert(prewarmStart >= 0);
const prewarmEnd = submitSource.indexOf("});", prewarmStart);
const expression = submitSource.slice(prewarmStart + 2, prewarmEnd + 1);
const values = new Proxy({}, {
  has: () => true,
  get: (target, key) => key === "n" ? () => native :
    key === "lL" ? scope(targets[0].clientThreadId) : undefined,
});
const prewarm = new Function("values", `with(values){return(${expression})}`)(values);
assert.equal(prewarm.submitDisabled, true);
const prewarmFunction = extractFunction(submitSource,
  "function(e){let t,n,o,r,i,l,s,d,m,h,p=(0,a.c)(82),{activeCollaborationMode:");
function runPrewarm(disabled) {
  let produced = 0;
  let cleared = 0;
  const reservation = { clear: () => cleared++, schedule: callback => callback() };
  const prewarmHook = prewarmFunction.runInNewContext({
    a: { c: size => Array(size).fill(Symbol.for("react.memo_cache_sentinel")) },
    c: { gh: () => ({}), _U: () => ({ data: {}, isPending: false }) },
    D: {}, R: { tn: () => ({ authMethod: "fixture" }) }, V: {},
    dt: { iW: () => ({ draftSettings: { isManuallyChanged: false } }) },
    hQ: { Y: () => false }, nO: { K: value => value },
    f: { useEffect: callback => callback() }, i3: { OX: () => false },
    le: { V: () => ({ prewarmConversation: () => produced++ }) },
  });
  prewarmHook({ ...prewarm, hasText: true, composerMode: "local",
    localThreadPrewarmReservation: reservation, getExecutionBlockReason: noop,
    submitDisabled: disabled });
  return { produced, cleared };
}
const targetPrewarm = runPrewarm(prewarm.submitDisabled);
const ordinaryPrewarm = runPrewarm(false);
assert.equal(targetPrewarm.produced, 0);
assert(targetPrewarm.cleared > 0);
assert(ordinaryPrewarm.produced > 0);
const guarded = native.registerNativeTargetDraft({
  hostId: "remote:target", projectId: "project-a",
  operationId: "019a1111-2222-4333-8444-777777777777",
});
const guardedScope = scope(guarded.clientThreadId);
projects.get("project-a").metadata = {
  "codex.chatGptBacking": JSON.stringify({ accountId: "other", userId: "user" }),
};
await assert.rejects(native.resolveNativeTargetDraft(guardedScope, guarded), /another native account/);
projects.get("project-a").metadata = {};
await native.resolveNativeTargetDraft(guardedScope, guarded);
pendingDeletions["remote:target:/target-home"] = { legacy: { projectId: "project-a" } };
await assert.rejects(native.resolveNativeTargetDraft(guardedScope, guarded), /pending deletion/);
delete pendingDeletions["remote:target:/target-home"];
overrides.SPnB.chatGptAuthenticatedAccountId$.initial = "changed";
await assert.rejects(native.resolveNativeTargetDraft(scope(targets[1].clientThreadId), targets[1]),
  /native account changed/);
overrides.SPnB.chatGptAuthenticatedAccountId$.initial = "account";
const firstTurnTarget = native.registerNativeTargetDraft({
  hostId: "remote:target", projectId: "project-a",
  operationId: "019a1111-2222-4333-8444-999999999999",
});
const firstTurnScope = scope(firstTurnTarget.clientThreadId);
await native.resolveNativeTargetDraft(firstTurnScope, firstTurnTarget);
native.nativeTargetAccepted(firstTurnTarget, "original-thread");
overrides.SPnB.chatGptAuthenticatedAccountId$.initial = "changed";
await assert.rejects(native.nativeTargetBeforeFirstTurn(firstTurnScope, firstTurnTarget),
  /original-thread.*native account changed/);
overrides.SPnB.chatGptAuthenticatedAccountId$.initial = "account";
await assert.rejects(native.nativeTargetBeforeFirstTurn(firstTurnScope, firstTurnTarget),
  /original-thread.*native account changed/);
console.log(JSON.stringify({ passed: true,
  realModules: ["V3rO", "HSUP", "aVy5", "e1yv", "QX7Q", "_D", "TSah", "xz.prepareStart"],
  targetSubmitReached: true, targetPrewarmDisabled: true,
  projects: expectedParams.map(params => params.projectId), writes: 0 }));
