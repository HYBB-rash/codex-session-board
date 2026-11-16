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
const requests = [];
const runtime = factory({
  createId: () => "client-new-thread:random",
  scopeId: scope => scope.id,
  existing: () => undefined,
  request: async (scope, hostId, method, params) => {
    requests.push({ hostId, method, params });
    if (method === "project/read") {
      if (!projects.has(params.projectId)) throw Error("Deleted");
      return { project: structuredClone(projects.get(params.projectId)) };
    }
    return { thread: threads.get(params.threadId) };
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
await runtime.verify({}, first, "thread-a");
await assert.rejects(runtime.resolve({}, first), /already created thread thread-a/);
threads.get("thread-a").projectId = null;
await assert.rejects(runtime.verify({}, first, "thread-a"), /thread-a.*lost/);
projects.get(second.projectId).roots.reverse();
await assert.rejects(runtime.resolve({}, second), /roots changed/);
projects.get(second.projectId).roots.reverse();
await assert.rejects(runtime.resolve({}, second), /roots changed/);
projects.delete(first.projectId);
await assert.rejects(runtime.beforeFirstTurn({}, first), /Deleted/);
assert(requests.every(request => ["project/read", "thread/read"].includes(request.method)));
assert(requests.every(request => request.hostId === route.hostId));
console.log(JSON.stringify({ passed: true, concurrentProjects: 2, writes: 0 }));
