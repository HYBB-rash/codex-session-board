import assert from "node:assert/strict";
import {
    closeSync,
    mkdirSync,
    mkdtempSync,
    openSync,
    realpathSync,
    rmSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test from "node:test";
import {
    assertFrameworkDigest,
    embeddedFramework,
    integrityDigest,
    sentinel,
} from "./mac-integrity.js";

test("framework detection finds one enabled digest across a read boundary", (context) => {
    const bundle = mkdtempSync(path.join(tmpdir(), "enhancer-mac-integrity-"));
    context.after(() => rmSync(bundle, { recursive: true, force: true }));
    const framework = path.join(bundle, "Contents", "Frameworks", "Test Framework.framework");
    mkdirSync(framework, { recursive: true });
    const binary = path.join(framework, "Test Framework");
    const slot = Buffer.alloc(sentinel.length + 34);
    sentinel.copy(slot);
    const padding = Buffer.alloc(1024 * 1024 - 12);
    writeFileSync(binary, Buffer.concat([padding, slot]));
    assert.equal(embeddedFramework(bundle), null);
    slot[sentinel.length] = 1;
    slot[sentinel.length + 1] = 1;
    writeFileSync(binary, Buffer.concat([padding, slot]));
    assert.equal(embeddedFramework(bundle), realpathSync(binary));
    writeFileSync(binary, Buffer.concat([padding, slot, slot]));
    assert.throws(() => embeddedFramework(bundle), /multiple enabled ASAR digest slots/);
});

test("the framework digest uses Electron's dictionary hash and the arm64 section", (context) => {
    const directory = mkdtempSync(path.join(tmpdir(), "enhancer-mac-section-"));
    context.after(() => rmSync(directory, { recursive: true, force: true }));
    const binary = path.join(directory, "Framework");
    const digest = integrityDigest({
        "Resources/app.asar": {
            algorithm: "SHA256",
            hash: "463dc668e2650299706a6820a28ab5e45c12cf93d65239c2fe8a7c07ad834b44",
        },
    });
    assert.equal(
        digest.toString("hex"),
        "9a16087c1bfc2ef12c35f39b170d57acd36f81c7f3a2e9c6ac8b3ffaf5f99cea",
    );
    const slotOffset = 184;
    const bytes = Buffer.alloc(slotOffset + sentinel.length + 34);
    bytes.writeUInt32LE(0xfeedfacf, 0);
    bytes.writeUInt32LE(0x0100000c, 4);
    bytes.writeUInt32LE(1, 16);
    bytes.writeUInt32LE(152, 20);
    bytes.writeUInt32LE(0x19, 32);
    bytes.writeUInt32LE(152, 36);
    bytes.write("__DATA_CONST", 40);
    bytes.writeUInt32LE(1, 96);
    bytes.write("__asar_integrity", 104);
    bytes.writeBigUInt64LE(BigInt(sentinel.length + 34), 144);
    bytes.writeUInt32LE(slotOffset, 152);
    sentinel.copy(bytes, slotOffset);
    bytes[slotOffset + sentinel.length] = 1;
    bytes[slotOffset + sentinel.length + 1] = 1;
    digest.copy(bytes, slotOffset + sentinel.length + 2);
    writeFileSync(binary, bytes);
    const file = openSync(binary, "r");
    try {
        assert.equal(assertFrameworkDigest(file, digest), slotOffset + sentinel.length + 2);
        assert.throws(() => assertFrameworkDigest(file, Buffer.alloc(32)), /does not match/);
    } finally {
        closeSync(file);
    }
});
