import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import test from "node:test";
import * as vm from "node:vm";
import { createArchive, integrityFor, openArchive, patchEntry, readEntry, type AsarNode } from "./asar.js";
import { loaderStub } from "./constants.js";

function fileIntegrityHash(node: AsarNode, entryPath: string): string {
  if ("files" in node || node.integrity === undefined) {
    throw new Error(`asar entry has no integrity: ${entryPath}`);
  }
  return node.integrity.hash;
}

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
    const mainNode = before.header.files["main.js"];
    if (mainNode === undefined) throw new Error("main.js is missing from the archive header");
    assert.equal(integrityFor(entry).hash, fileIntegrityHash(mainNode, "main.js"));
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
    const mainNode = after.header.files["main.js"];
    if (mainNode === undefined) throw new Error("main.js is missing from the archive header");
    assert.equal(fileIntegrityHash(mainNode, "main.js"), integrityFor(patched).hash);
  } finally {
    fs.closeSync(after.fd);
  }
  fs.rmSync(directory, { recursive: true, force: true });
});

test("the installed Codex archive header matches package.json", { skip: !fs.existsSync("/opt/codex-desktop/resources/app.asar") }, () => {
  const archive = openArchive("/opt/codex-desktop/resources/app.asar");
  try {
    const manifest: unknown = JSON.parse(readEntry(archive, "package.json").toString("utf8"));
    if (typeof manifest !== "object" || manifest === null || !("main" in manifest) || typeof manifest.main !== "string") {
      throw new Error("package.json main is missing");
    }
    assert.equal(typeof manifest.main, "string");
    const entry = readEntry(archive, manifest.main);
    assert.equal(entry.length > 0, true);
  } finally {
    fs.closeSync(archive.fd);
  }
});
