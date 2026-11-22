import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test from "node:test";
import { assertPatchableMacBundle, sentinel } from "./mac-integrity.js";

test("a macOS framework with an embedded ASAR digest is refused before patching", (context) => {
    const bundle = mkdtempSync(path.join(tmpdir(), "enhancer-mac-integrity-"));
    context.after(() => rmSync(bundle, { recursive: true, force: true }));
    const framework = path.join(bundle, "Contents", "Frameworks", "Test Framework.framework");
    mkdirSync(framework, { recursive: true });
    const binary = path.join(framework, "Test Framework");
    const slot = Buffer.alloc(sentinel.length + 34);
    sentinel.copy(slot);
    const padding = Buffer.alloc(1024 * 1024 - 12);
    writeFileSync(binary, Buffer.concat([padding, slot]));
    assert.doesNotThrow(() => assertPatchableMacBundle(bundle, "1.0.0"));
    slot[sentinel.length] = 1;
    slot[sentinel.length + 1] = 1;
    writeFileSync(binary, Buffer.concat([padding, slot]));
    assert.throws(
        () => assertPatchableMacBundle(bundle, "1.0.0"),
        /1.0.0 embeds the app.asar integrity dictionary digest in Test Framework/,
    );
});
