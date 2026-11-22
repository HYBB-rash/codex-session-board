"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const assert = require("node:assert/strict");
const { createArchive, integrityFor, openArchive, patchEntry, readEntry } = require("./asar");
const { loaderStub } = require("./constants");

test("loader stub is valid JavaScript before the original program", () => {
  const script = new vm.Script(`${loaderStub()}(() => {})\n`);
  assert.equal(typeof script, "object");
});

test("archive round trip and patch keep the untouched file", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "codex-enhancer-asar-"));
  const archivePath = path.join(directory, "app.asar");
  const entry = Buffer.from("(() => { globalThis.started = true; })();\n");
  const manifest = Buffer.from('{"main":"main.js"}\n');
  createArchive(archivePath, [
    { path: "package.json", data: manifest },
    { path: "main.js", data: entry },
  ]);

  const before = openArchive(archivePath);
  try {
    assert.deepEqual(readEntry(before, "package.json"), manifest);
    assert.deepEqual(readEntry(before, "main.js"), entry);
    assert.equal(integrityFor(entry).hash, before.header.files["main.js"].integrity.hash);
  } finally {
    fs.closeSync(before.fd);
  }

  const stub = Buffer.from("/*patched*/\n");
  const result = patchEntry(archivePath, "main.js", (current) => Buffer.concat([stub, current]));
  assert.equal(result.changed, true);

  const after = openArchive(archivePath);
  try {
    assert.deepEqual(readEntry(after, "package.json"), manifest);
    const patched = readEntry(after, "main.js");
    assert.deepEqual(patched.subarray(0, stub.length), stub);
    assert.deepEqual(patched.subarray(stub.length), entry);
    assert.equal(after.header.files["main.js"].integrity.hash, integrityFor(patched).hash);
  } finally {
    fs.closeSync(after.fd);
  }
  fs.rmSync(directory, { recursive: true, force: true });
});

test("the installed Codex archive header matches package.json", { skip: !fs.existsSync("/opt/codex-desktop/resources/app.asar") }, () => {
  const archive = openArchive("/opt/codex-desktop/resources/app.asar");
  try {
    const manifest = JSON.parse(readEntry(archive, "package.json").toString("utf8"));
    assert.equal(typeof manifest.main, "string");
    const entry = readEntry(archive, manifest.main);
    assert.equal(entry.length > 0, true);
  } finally {
    fs.closeSync(archive.fd);
  }
});
