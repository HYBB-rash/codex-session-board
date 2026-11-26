import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test from "node:test";
import { installLauncher } from "./launcher.js";

test("the launcher is a signed app bundle that runs the official start command", {
    skip: process.platform !== "darwin",
}, (context) => {
    const root = mkdtempSync(path.join(tmpdir(), "codex-board-launcher-"));
    context.after(() => rmSync(root, { recursive: true, force: true }));
    const destination = path.join(root, "Codex 看板.app");
    const result = installLauncher({
        destination,
        startArgs: ["--app", "/Applications/ChatGPT.app"],
        iconSource: path.join(root, "missing.icns"),
    });
    assert.equal(result.path, destination);
    const script = readFileSync(path.join(destination, "Contents", "MacOS", "launcher"), "utf8");
    assert.match(script, /'official' 'start' '--app' '\/Applications\/ChatGPT\.app'/);
    assert.match(
        readFileSync(path.join(destination, "Contents", "Info.plist"), "utf8"),
        /<key>LSUIElement<\/key><true\/>/,
    );
    assert.equal(existsSync(path.join(destination, "Contents", "PkgInfo")), true);
    execFileSync("/usr/bin/codesign", ["--verify", "--deep", "--strict", destination], {
        stdio: "pipe",
    });
});
