import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import {
    releaseManifestName,
    releasePlatform,
    verifyRelease,
    versionManifest,
} from "./release-manifest.js";
import { activateRelease, activeRelease, rollbackRelease } from "./releases.js";

function fixture(directory: string, version: string): string {
    const root = path.join(directory, version);
    const dist = path.join(root, "dist");
    const files = ["bin/codex-enhancer.js", "runtime/main.js", "runtime/preload.js"];
    for (const file of files) {
        const target = path.join(dist, file);
        mkdirSync(path.dirname(target), { recursive: true });
        writeFileSync(target, `exports.version = "${version}";`);
    }
    const manifest = versionManifest(dist, version, releasePlatform());
    writeFileSync(path.join(root, releaseManifestName), JSON.stringify(manifest));
    return root;
}

function scratch(context: TestContext): string {
    const directory = mkdtempSync(path.join(tmpdir(), "enhancer-release-test-"));
    context.after(() => rmSync(directory, { recursive: true, force: true }));
    return directory;
}

test("activation and rollback retain data and the previous immutable release", (context) => {
    const directory = scratch(context);
    const releases = path.join(directory, "state", "releases");
    const first = fixture(directory, "0.3.0");
    const second = fixture(directory, "0.3.1");
    assert.equal(activateRelease(first, releases).version, "0.3.0");
    assert.equal(activateRelease(second, releases).version, "0.3.1");
    rmSync(second, { recursive: true });
    assert.equal(activeRelease(releases)?.version, "0.3.1");
    assert.equal(rollbackRelease(releases).version, "0.3.0");
    assert.equal(activeRelease(releases)?.version, "0.3.0");
    assert.equal(rollbackRelease(releases).version, "0.3.1");
});

test("invalid candidate leaves selection available for the next attempt", (context) => {
    const directory = scratch(context);
    const releases = path.join(directory, "releases");
    const first = fixture(directory, "0.3.0");
    const second = fixture(directory, "0.3.1");
    const corrupt = path.join(directory, "corrupt");
    cpSync(second, corrupt, { recursive: true });
    writeFileSync(path.join(corrupt, "dist", "runtime", "main.js"), "tampered");
    activateRelease(first, releases);
    assert.throws(() => activateRelease(corrupt, releases), /SHA-256/);
    assert.equal(activeRelease(releases)?.version, "0.3.0");
    assert.equal(activateRelease(second, releases).version, "0.3.1");
});

test("platform mismatch and symlink entries are rejected", (context) => {
    const directory = scratch(context);
    const root = fixture(directory, "0.3.0");
    const opposite = releasePlatform() === "linux-x86_64" ? "macos-arm64" : "linux-x86_64";
    assert.throws(() => verifyRelease(root, opposite), /expected/);
    symlinkSync(path.join(root, "release.json"), path.join(root, "dist", "foreign"));
    assert.throws(() => verifyRelease(root), /invalid release file/);
});

test("a live update owner prevents concurrent changes and a departed owner can be replaced", (context) => {
    const directory = scratch(context);
    const releases = path.join(directory, "releases");
    const lock = path.join(releases, ".activating");
    const candidate = fixture(directory, "0.3.0");
    mkdirSync(lock, { recursive: true });
    const present = path.join(lock, `${process.pid}-1234`);
    writeFileSync(present, "");
    assert.throws(() => activateRelease(candidate, releases), /update or rollback is in progress/);
    rmSync(present);
    writeFileSync(path.join(lock, "2147483647-1234"), "");
    assert.equal(activateRelease(candidate, releases).version, "0.3.0");
});
