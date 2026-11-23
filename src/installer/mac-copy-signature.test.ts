import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import { createArchive, sha256File } from "./asar.js";

const executable = path.join("Contents", "MacOS", "Fixture");
const archive = path.join("Contents", "Resources", "app.asar");
const plist = path.join("Contents", "Info.plist");
const envelope = path.join("Contents", "_CodeSignature", "CodeResources");

function verify(bundle: string): void {
    execFileSync("/usr/bin/codesign", ["--verify", "--deep", "--strict", bundle]);
}

function bundleFingerprint(bundle: string): string[] {
    return [executable, archive, plist, envelope].map((file) =>
        sha256File(path.join(bundle, file)),
    );
}

function createBundle(stock: string): void {
    for (const directory of ["Frameworks", "MacOS", "Resources"]) {
        fs.mkdirSync(path.join(stock, "Contents", directory), { recursive: true });
    }
    fs.copyFileSync("/usr/bin/true", path.join(stock, executable));
    fs.writeFileSync(
        path.join(stock, plist),
        '<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict>' +
            "<key>CFBundleExecutable</key><string>Fixture</string>" +
            "<key>CFBundleIdentifier</key><string>local.enhancer-signature-fixture</string>" +
            "<key>CFBundlePackageType</key><string>APPL</string>" +
            "<key>CFBundleShortVersionString</key><string>1.0</string></dict></plist>",
    );
    createArchive(path.join(stock, archive), [
        { path: "package.json", data: '{"main":"main.js"}' },
        { path: "main.js", data: "void 0;\n" },
    ]);
    execFileSync("/usr/bin/codesign", ["--force", "--sign", "-", stock]);
    verify(stock);
}

function fixture(context: TestContext) {
    const root = fs.mkdtempSync(path.join(fs.realpathSync(tmpdir()), "enhancer-mac-signature-"));
    const stock = path.join(root, "Stock.app");
    const app = path.join(root, "state", "Codex Enhancer.app");
    const installReceipt = path.join(root, "state", "install.json");
    const savedSource = process.env.CODEX_ENHANCER_SOURCE;
    process.env.CODEX_ENHANCER_SOURCE = stock;
    context.after(() => {
        if (savedSource === undefined) delete process.env.CODEX_ENHANCER_SOURCE;
        else process.env.CODEX_ENHANCER_SOURCE = savedSource;
        fs.rmSync(root, { recursive: true, force: true });
    });
    createBundle(stock);
    const compiled = path.join(root, "dist", "installer");
    fs.cpSync(__dirname, compiled, { recursive: true });
    const load = createRequire(__filename);
    const install = load(path.join(compiled, "install.js")) as typeof import("./install.js");
    const restore = load(path.join(compiled, "restore.js")) as typeof import("./restore.js");
    const fsModule = load("node:fs") as typeof fs;
    return {
        install,
        restore,
        fsModule,
        installReceipt,
        original: bundleFingerprint(stock),
        stock,
        app,
    };
}

test("a Mac copy restores its original signature after a failed install and on uninstall", {
    skip: process.platform !== "darwin",
}, (context) => {
    const { install, restore, fsModule, installReceipt, original, stock, app } = fixture(context);
    const rename = fsModule.renameSync;
    const failure = new Error("injected write receipt failure");
    const failRename: typeof rename = (from, to) => {
        if (to === installReceipt) throw failure;
        rename(from, to);
    };
    const failInstall = context.mock.method(fsModule, "renameSync", failRename);
    assert.throws(install.install, (error) => error === failure);
    failInstall.mock.restore();
    assert.deepEqual(bundleFingerprint(app), original);
    verify(app);

    const result = install.install();
    const patched = bundleFingerprint(app);
    assert.notEqual(patched[0], original[0]);
    assert.notEqual(patched[1], original[1]);
    const backup = `${result.state.backup.archive}.mac-signature`;
    const displaced = `${backup}.temporarily-missing`;
    rename(backup, displaced);
    assert.throws(restore.restore, /macOS signature backup is missing/);
    assert.deepEqual(bundleFingerprint(app), patched);
    assert.equal(install.readState()?.status, "installed");
    rename(displaced, backup);

    assert.equal(restore.restore().state.status, "removed");
    verify(app);
    assert.deepEqual(bundleFingerprint(app), original);
    assert.deepEqual(bundleFingerprint(stock), original);
});
