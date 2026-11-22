import assert from "node:assert/strict";
import {
    cpSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    rmSync,
    symlinkSync,
    writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import { sha256File } from "./asar.js";
import {
    inPlaceReleaseProtocol,
    type ReleasePlatform,
    releaseManifestName,
    releasePlatform,
    verifyInPlaceRelease,
    verifyRelease,
    versionManifest,
} from "./release-manifest.js";
import { activateRelease, activeRelease, rollbackRelease } from "./releases.js";

const load = createRequire(__filename);

function fixture(
    directory: string,
    version: string,
    {
        platform = releasePlatform(),
        inPlaceProtocol,
    }: { platform?: ReleasePlatform; inPlaceProtocol?: number } = {},
): string {
    const root = path.join(directory, version);
    const dist = path.join(root, "dist");
    const files = ["bin/codex-enhancer.js", "runtime/main.js", "runtime/preload.js"];
    for (const file of files) {
        const target = path.join(dist, file);
        mkdirSync(path.dirname(target), { recursive: true });
        writeFileSync(target, `exports.version = "${version}";`);
    }
    const manifest = versionManifest(dist, version, platform);
    if (inPlaceProtocol !== undefined) manifest.inPlaceProtocol = inPlaceProtocol;
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

test("in-place verification requires a supported Linux protocol", (context) => {
    const directory = scratch(context);
    const linux = { platform: "linux-x86_64" } as const;
    const legacy = fixture(directory, "0.3.23", linux);
    const supported = fixture(directory, "0.3.24", {
        ...linux,
        inPlaceProtocol: inPlaceReleaseProtocol,
    });
    const future = fixture(directory, "0.3.25", {
        ...linux,
        inPlaceProtocol: inPlaceReleaseProtocol + 1,
    });
    const mac = fixture(directory, "0.3.26", {
        platform: "macos-arm64",
        inPlaceProtocol: inPlaceReleaseProtocol,
    });
    assert.equal(verifyRelease(legacy, linux.platform).version, "0.3.23");
    assert.equal(verifyRelease(future, linux.platform).version, "0.3.25");
    assert.equal(verifyInPlaceRelease(supported).version, "0.3.24");
    assert.throws(() => verifyInPlaceRelease(legacy), /in-place installation protocol/);
    assert.throws(() => verifyInPlaceRelease(future), /in-place installation protocol/);
    assert.throws(() => verifyInPlaceRelease(mac), /expected linux-x86_64/);
    writeFileSync(path.join(supported, "dist/runtime/main.js"), "tampered");
    assert.throws(() => verifyInPlaceRelease(supported), /SHA-256/);
});

test("in-place activation and rollback reject copy-only releases before switching", (context) => {
    const directory = scratch(context);
    const store = path.join(directory, "releases");
    const receipt = load("./in-place-receipt.js") as typeof import("./in-place-receipt.js");
    const target = load("./in-place-target.js") as typeof import("./in-place-target.js");
    context.mock.method(receipt, "inPlaceReleaseStore", () => store);
    const stageCheck = context.mock.method(target, "assertSystemPath", () => {});
    const legacy = fixture(directory, "0.3.23", { platform: "linux-x86_64" });
    const inPlace = { platform: "linux-x86_64", inPlaceProtocol: inPlaceReleaseProtocol } as const;
    const first = fixture(directory, "0.3.24", inPlace);
    const second = fixture(directory, "0.3.25", inPlace);
    const seed = (packageRoot: string) => {
        const id = sha256File(path.join(packageRoot, releaseManifestName));
        cpSync(packageRoot, path.join(store, id), { recursive: true });
        return id;
    };
    const previous = seed(legacy);
    const current = seed(first);
    seed(second);
    const pointerFile = path.join(store, "active.json");
    const pointer = `${JSON.stringify({ current, previous })}\n`;
    writeFileSync(pointerFile, pointer);

    assert.equal(activeRelease(store)?.version, "0.3.24");
    assert.throws(() => activateRelease(legacy, store), /in-place installation protocol/);
    assert.equal(readFileSync(pointerFile, "utf8"), pointer);
    assert.throws(() => rollbackRelease(store), /in-place installation protocol/);
    assert.equal(readFileSync(pointerFile, "utf8"), pointer);
    assert.equal(stageCheck.mock.callCount(), 0);

    assert.equal(activateRelease(second, store).version, "0.3.25");
    assert.equal(activeRelease(store)?.version, "0.3.25");
    assert.equal(rollbackRelease(store).version, "0.3.24");
    assert.equal(activeRelease(store)?.version, "0.3.24");

    writeFileSync(pointerFile, JSON.stringify({ current: previous, previous: null }));
    assert.throws(() => activeRelease(store), /in-place installation protocol/);
    assert.equal(activateRelease(first, store).version, "0.3.24");
    assert.equal(activeRelease(store)?.version, "0.3.24");
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
