import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const load = async name => vm.runInNewContext(
  `(${await readFile(new URL(`./native_new/${name}`, import.meta.url), "utf8")})`,
);
const factory = await load("target.js");
const parse = await load("route.js");
const plain = value => JSON.parse(JSON.stringify(value));
const route = { hostId: "remote:a", projectId: "canonical-a",
  operationId: "019a1111-2222-4333-8444-555555555555" };
const url = new URL("codex://threads/new-target?" + new URLSearchParams(route));
assert.deepEqual(plain(parse(url)), { kind: "targetNewThread", ...route });
for (const suffix of ["&cwd=/other", "&prompt=send", "&operationId=duplicate", "#fragment"])
  assert.equal(parse(new URL(url + suffix)), null);
assert.equal(parse(new URL(url.toString().replace(route.operationId, "bad"))), null);
const projects = new Map(["canonical-a", "canonical-b"].map(id => [
  id, { id, roots: [{ path: "/same" }, { path: "/second" }] },
]));
const threads = new Map();
const unpublished = new Map();
const requests = [];
const account = { accountId: "account", userId: "user" };
const runtime = factory({
  createId: () => "client-new-thread:random",
  scopeId: scope => scope.id,
  existing: () => undefined,
  readContext: () => account,
  wait: async () => {},
  request: async (scope, hostId, method, params) => {
    requests.push({ hostId, method, params });
    if (method === "project/read") {
      if (!projects.has(params.projectId)) throw Error("Deleted");
      return { project: structuredClone(projects.get(params.projectId)) };
    }
    const thread = threads.get(params.threadId);
    if ((unpublished.get(params.threadId) ?? 0) > 0) {
      unpublished.set(params.threadId, unpublished.get(params.threadId) - 1);
      return { thread: { ...thread, projectId: null } };
    }
    return { thread };
  },
});
const first = runtime.register(route);
const second = runtime.register({ ...route, projectId: "canonical-b",
  operationId: "019a1111-2222-4333-8444-666666666666" });
assert.equal(runtime.register(route), first);
assert.notEqual(first.clientThreadId, second.clientThreadId);
assert.equal(runtime.get({ id: first.clientThreadId }), first);
assert.throws(() => runtime.register({ ...route, projectId: "other" }), /another project/);
const workspaces = await Promise.all([first, second].map(target => runtime.resolve({}, target)));
assert.deepEqual(workspaces.map(plain), [0, 1].map(() => ({
  cwd: "/same", workspaceRoots: ["/same", "/second"], projectlessOutputDirectory: null,
})));
for (const target of [first, second]) {
  const inputs = await runtime.creationInputs({}, route.hostId, {
    ...workspaces[0], nativeTargetDraft: plain(target),
  }, {});
  assert.equal(inputs.projectId, target.projectId);
}
await assert.rejects(runtime.creationInputs({}, "other-host", {
  ...workspaces[0], nativeTargetDraft: first,
}, {}), /host or assignment/);
await assert.rejects(runtime.creationInputs({}, route.hostId, {
  ...workspaces[0], nativeTargetDraft: first, projectAssignment: null,
}, {}), /host or assignment/);
runtime.accepted(first, "thread-a");
threads.set("thread-a", { id: "thread-a", projectId: first.projectId });
await runtime.beforeFirstTurn({}, first);
unpublished.set("thread-a", 2);
await runtime.verify({}, first, "thread-a");
assert.equal(unpublished.get("thread-a"), 0);
await assert.rejects(runtime.resolve({}, first), /already created thread thread-a/);
threads.get("thread-a").projectId = null;
const beforeUnconfirmed = requests.length;
await assert.rejects(runtime.verify({}, first, "thread-a"), /thread-a.*unconfirmed/);
assert.equal(requests.length - beforeUnconfirmed, 20);
threads.get("thread-a").projectId = "different-project";
const beforeMismatch = requests.length;
await assert.rejects(runtime.verify({}, first, "thread-a"), /thread-a.*different target/);
assert.equal(requests.length - beforeMismatch, 1);
threads.get("thread-a").id = "another-thread";
await assert.rejects(runtime.verify({}, first, "thread-a"), /thread-a.*readback ID changed/);
await assert.rejects(runtime.verify({}, first, "another-thread"), /bound to thread thread-a/);
assert.equal(first.conversationId, "thread-a");
projects.get(second.projectId).roots.reverse();
await assert.rejects(runtime.resolve({}, second), /roots changed/);
projects.get(second.projectId).roots.reverse();
await assert.rejects(runtime.resolve({}, second), /roots changed/);
await assert.rejects(runtime.beforeFirstTurn({}, second), /roots changed/);
projects.delete(first.projectId);
await assert.rejects(runtime.beforeFirstTurn({}, first), /Deleted/);
const ownerTarget = runtime.register({ ...route,
  operationId: "019a1111-2222-4333-8444-777777777777" });
projects.set(route.projectId, { id: route.projectId, roots: [{ path: "/same" }],
  metadata: { "codex.chatGptBacking": JSON.stringify({ accountId: "other", userId: "user" }) } });
await assert.rejects(runtime.resolve({}, ownerTarget), /another native account/);
projects.get(route.projectId).metadata = {};
await runtime.resolve({}, ownerTarget);
account.accountId = "other";
await assert.rejects(runtime.resolve({}, ownerTarget), /native account changed/);
account.accountId = "account";
await assert.rejects(runtime.resolve({}, ownerTarget), /native account changed/);
const firstTurnTarget = runtime.register({ ...route,
  operationId: "019a1111-2222-4333-8444-999999999999" });
await runtime.resolve({}, firstTurnTarget);
runtime.accepted(firstTurnTarget, "thread-before-first-turn");
account.accountId = "other";
await assert.rejects(runtime.beforeFirstTurn({}, firstTurnTarget), /native account changed/);
account.accountId = "account";
await assert.rejects(runtime.beforeFirstTurn({}, firstTurnTarget), /native account changed/);
const rootsTarget = runtime.register({ ...route,
  operationId: "019a1111-2222-4333-8444-aaaaaaaaaaaa" });
await runtime.resolve({}, rootsTarget);
runtime.accepted(rootsTarget, "original-root-thread");
projects.get(route.projectId).roots.push({ path: "/changed" });
await assert.rejects(runtime.beforeFirstTurn({}, rootsTarget), /original-root-thread.*changed/);
projects.get(route.projectId).roots.pop();
await assert.rejects(runtime.beforeFirstTurn({}, rootsTarget), /original-root-thread.*changed/);
const pendingTarget = runtime.register({ ...route,
  operationId: "019a1111-2222-4333-8444-888888888888" });
account.pendingDeletion = true;
await assert.rejects(runtime.resolve({}, pendingTarget), /pending deletion/);
assert(requests.every(request => ["project/read", "thread/read"].includes(request.method)));
assert(requests.every(request => request.hostId === route.hostId));
console.log(JSON.stringify({ passed: true, concurrentProjects: 2, writes: 0 }));
