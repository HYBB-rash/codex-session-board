import assert from "node:assert/strict";
import * as fs from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import { createArchive, patchEntry, sha256File } from "./asar.js";
import { LINK_NAME, PATCH_VERSION } from "./constants.js";

const load = createRequire(__filename);
const fsModule = load("node:fs") as typeof fs;

function mockMacSigning(context: TestContext, compiled: string): void {
    const copies = load(path.join(compiled, "copy.js")) as typeof import("./copy.js");
    context.mock.method(copies, "backupMacMetadata", () => {});
    context.mock.method(copies, "assertMacMetadataAvailable", () => {});
    const applyCopyIdentifier: typeof copies.signPatchedCopy = (copy) => {
        if (copy.platform === "darwin") copies.setIsolatedMacBundleIdentifier(copy.appDir);
    };
    context.mock.method(copies, "signPatchedCopy", applyCopyIdentifier);
    context.mock.method(copies, "restoreMacMetadata", () => {});
}

function fixture(context: TestContext) {
    const root = fs.mkdtempSync(path.join(fs.realpathSync(tmpdir()), "enhancer-install-"));
    const mac = process.platform === "darwin";
    const source = path.join(root, mac ? "Stock.app" : "stock");
    const resources = path.join(source, mac ? "Contents/Resources" : "resources");
    const state = path.join(root, "state");
    const copy = path.join(state, mac ? "Codex Enhancer.app" : "test-app");
    const archive = path.join(resources, "app.asar");
    const target = path.join(copy, path.relative(source, archive));
    const link = path.join(copy, mac ? "Contents/Resources" : "", LINK_NAME);
    const receipt = path.join(state, "install.json");
    const savedSource = process.env.CODEX_ENHANCER_SOURCE;
    process.env.CODEX_ENHANCER_SOURCE = source;
    context.after(() => {
        if (savedSource === undefined) delete process.env.CODEX_ENHANCER_SOURCE;
        else process.env.CODEX_ENHANCER_SOURCE = savedSource;
        fs.rmSync(root, { recursive: true, force: true });
    });
    fs.mkdirSync(resources, { recursive: true });
    if (mac) {
        fs.mkdirSync(path.join(source, "Contents/Frameworks"));
        fs.mkdirSync(path.join(source, "Contents/MacOS"));
        fs.writeFileSync(path.join(source, "Contents/MacOS/Synthetic"), "");
        fs.writeFileSync(
            path.join(source, "Contents/Info.plist"),
            '<?xml version="1.0"?><plist version="1.0"><dict>' +
                "<key>CFBundleExecutable</key><string>Synthetic</string>" +
                "<key>CFBundleIdentifier</key><string>local.enhancer-install-fixture</string>" +
                "<key>CFBundleShortVersionString</key><string>1.0</string></dict></plist>",
        );
    } else {
        fs.writeFileSync(path.join(source, "ChatGPT"), "");
        fs.writeFileSync(path.join(source, "version"), "1.0");
        fs.writeFileSync(path.join(resources, "linux-package-metadata.json"), '{"version":"1.0"}');
        fs.writeFileSync(path.join(resources, "owl-electron-app.json"), "{}");
    }
    createArchive(archive, [
        { path: "package.json", data: Buffer.from('{"main":"main.js"}') },
        { path: "main.js", data: Buffer.from("void 0;\n") },
    ]);
    const compiled = path.join(root, "dist", "installer");
    fs.cpSync(__dirname, compiled, { recursive: true });
    const installer = load(path.join(compiled, "install.js")) as typeof import("./install.js");
    const uninstaller = load(path.join(compiled, "restore.js")) as typeof import("./restore.js");
    mockMacSigning(context, compiled);
    return { ...installer, ...uninstaller, archive, link, receipt, state, target };
}

function failReceiptRename(context: TestContext, receipt: string) {
    const rename = fsModule.renameSync;
    const failure = Object.assign(new Error("simulated receipt rename failure"), {
        code: "EACCES",
    });
    const failRename: typeof rename = (from, to) => {
        if (to === receipt) throw failure;
        rename(from, to);
    };
    const mock = context.mock.method(fsModule, "renameSync", failRename);
    return { failure, restore: () => mock.mock.restore() };
}

test("a failed first install restores the original archive and can be retried", (context) => {
    const app = fixture(context);
    const original = sha256File(app.archive);
    const fault = failReceiptRename(context, app.receipt);
    assert.throws(app.install, (error) => error === fault.failure);
    fault.restore();

    assert.equal(sha256File(app.archive), original);
    assert.equal(sha256File(app.target), original);
    assert.equal(fs.existsSync(app.link), false);
    assert.equal(app.readState(), null);
    assert.ok(fs.readdirSync(app.state).every((file) => !file.endsWith(".next")));

    const result = app.install();
    assert.equal(result.action, "patched");
    assert.equal(app.readState()?.status, "installed");
    assert.notEqual(sha256File(app.target), original);
    assert.equal(app.restore().archiveHash, original);
    assert.equal(app.install().action, "patched");
});

test("a failed loader upgrade restores the prior patched archive, link and receipt", (context) => {
    const app = fixture(context);
    const original = sha256File(app.archive);
    const initial = app.install().state;
    patchEntry(app.target, "main.js", (entry) =>
        Buffer.concat([entry, Buffer.from("/* previous loader */\n")]),
    );
    const oldHash = sha256File(app.target);
    const link = JSON.parse(fs.readFileSync(app.link, "utf8")) as { patchVersion: string };
    link.patchVersion = "0.2.0";
    const oldLink = Buffer.from(JSON.stringify(link));
    fs.writeFileSync(app.link, oldLink);
    app.writeState({
        ...initial,
        patchVersion: link.patchVersion,
        files: { archive: { ...initial.files.archive, patchedSha256: oldHash } },
    });
    const oldReceipt = fs.readFileSync(app.receipt);

    const fault = failReceiptRename(context, app.receipt);
    assert.throws(app.install, (error) => error === fault.failure);
    fault.restore();
    assert.equal(sha256File(app.archive), original);
    assert.equal(sha256File(app.target), oldHash);
    assert.deepEqual(fs.readFileSync(app.link), oldLink);
    assert.deepEqual(fs.readFileSync(app.receipt), oldReceipt);

    assert.equal(app.install().state.patchVersion, PATCH_VERSION);
    assert.notEqual(sha256File(app.target), oldHash);
    assert.equal(app.install().action, "unchanged");
    assert.equal(app.restore().archiveHash, original);
});
