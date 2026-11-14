import assert from "node:assert/strict";

export const fields = row => Object.fromEntries(
  ["id", "state", "note", "manual", "version", "inputVersion", "sectionVersion"].map(key => [key, row[key]]));

export function unchanged(current, owned) {
  assert(current && owned, "测试卡片缺失，拒绝写入或恢复");
  assert.deepEqual(fields(current), fields(owned), "卡片已被其他操作修改，拒绝覆盖");
}

export function stateWrite(request, owned, url) {
  assert.equal(request.url, new URL("/api/state", url).href);
  assert.equal(request.method, "POST");
  const body = JSON.parse(request.postData);
  assert.equal(body.id, owned.id, "拒绝操作其他卡片");
  for (const key of ["version", "inputVersion", "sectionVersion"])
    assert.equal(body[key], owned[key], `拒绝使用不同的${key}`);
  return body;
}
