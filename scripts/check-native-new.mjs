import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const directory = new URL("./native_new/", import.meta.url);
const load = async name => vm.runInNewContext(
  `(${await readFile(new URL(name, directory), "utf8")})`,
);
const parse = await load("route.js");
const open = await load("open.js");
const url = "codex://threads/new-remote?hostId=remote%3Aa&projectId=p%20%26%20%E4%B8%AD";
const route = JSON.parse(JSON.stringify(parse(new URL(url))));
assert.deepEqual(route, { kind: "remoteNewThread", hostId: "remote:a", projectId: "p & 中" });
for (const invalid of [
  url + "&prompt=send", url + "&hostId=remote:b", url + "#fragment",
  url.replace("codex:", "https:"), url.replace("remote%3Aa", "local"),
  url.replace("hostId=remote%3Aa", "hostId="), url.replace("new-remote", "new-remote/extra"),
]) assert.equal(parse(new URL(invalid)), null);
const events = [];
const project = { id: route.projectId, hostId: route.hostId, remotePath: "/remote/project" };
const dependencies = {
  route, projectsManager: { getRemoteProjects: () => [project] },
  selectProject: async selection => events.push(["select", selection]),
  announce: () => events.push(["announce"]),
  navigate: selection => events.push(["navigate", selection]),
};
await open(dependencies);
assert.deepEqual(JSON.parse(JSON.stringify(events)), [
  ["select", { type: "remote", projectId: route.projectId }],
  ["announce"], ["navigate", { type: "remote", projectId: route.projectId }],
]);
for (const projects of [
  [], [project, project], [project, { ...project, hostId: "remote:b" }],
  [{ ...project, hostId: "remote:b" }],
  [{ ...project, remotePath: "relative" }],
]) {
  events.length = 0;
  await assert.rejects(open({
    ...dependencies, projectsManager: { getRemoteProjects: () => projects },
  }));
  assert.deepEqual(events, []);
}
console.log(JSON.stringify({ passed: true, source: fileURLToPath(directory) }));
await import("./check-native-target.mjs");
