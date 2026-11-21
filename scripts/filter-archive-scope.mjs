import assert from "node:assert/strict";
import { until } from "./browser-actions.mjs";

const uuid = "00000000-0000-4000-8000-000000000081";
const hostLabel = "工作主机";
const hosts = ["host-a", "host-b"];
const session = (hostId, threadId, projectId, version) => ({
  id: "remote:" + JSON.stringify(["codex", hostId, threadId]), remote: true, hostId, hostLabel,
  title: "检查归档", projectId, cwd: "/example", state: "to_archive", canArchive: true,
  version, inputVersion: version + 4, sectionVersion: "section-" + version,
});
const paired = hosts.map((host, index) => session(host, uuid, host + "/project", 10 + index));
const otherProject = session(hosts[0], "00000000-0000-4000-8000-000000000082", "other", 20);

export const filterSnapshot = {
  fourColumns: true, states: {in_progress:"进行中", review:"待审阅", blocked:"已阻塞", to_archive:"待归档"},
  health: {remoteRegistry: hosts.map(hostId => ({hostId, hostLabel, enabled:true}))},
  sessions: [paired[1], otherProject, paired[0]],
  projectCatalog: {loaded:true, items: [...paired, otherProject].map(row => ({
    id:row.projectId, paths:[row.cwd], label:row === otherProject ? "其他项目" : "目标项目",
    remote:true, hostId:row.hostId, hostLabel,
  }))},
};

export async function checkFilterArchiveScope(evaluate, writes) {
  const select = (id, value) => evaluate(`(() => {
    const control = document.getElementById(${JSON.stringify(id)});
    control.value = ${JSON.stringify(value)};
    control.dispatchEvent(new Event("change", {bubbles:true}));
  })()`);
  const options = await evaluate('Array.from(document.querySelector("#host-filter").options, option => option.textContent)');
  for (const row of paired) assert(options.includes(hostLabel + " · " + row.hostId));
  const cards = () => evaluate('Array.from(document.querySelectorAll(".card"), card => card.dataset.id)');
  for (const row of paired) {
    await select("host-filter", "host:" + row.hostId);
    await select("project-filter", "project:" + row.projectId);
    assert.deepEqual(await cards(), [row.id]);
    const start = writes.length;
    await evaluate('document.querySelector("#column-to_archive [data-archive-all]").click()');
    await until(() => writes.slice(start).some(write => write.path === "/api/archive-batches/finish"),
      "所选主机的归档批次未完成");
    const entries = writes.slice(start);
    const registrations = entries.filter(write => write.path === "/api/archive-batches");
    assert.equal(registrations.length, 1);
    const revision = {id:row.id, version:row.version, inputVersion:row.inputVersion, sectionVersion:row.sectionVersion};
    const batch = registrations[0].body;
    assert(batch.id);
    assert.deepEqual(batch.items, [{...revision, title:row.title}]);
    assert.deepEqual(entries.filter(write => write.path === "/api/archive").map(write => write.body),
      [{...revision, batchId:batch.id}]);
    await until(() => evaluate('!document.querySelector("#host-filter").disabled'), "归档后的筛选控件仍被锁定");
  }
}
