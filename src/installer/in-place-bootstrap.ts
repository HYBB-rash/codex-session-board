import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import type { App } from "electron";
import { sha256File } from "./asar.js";
import { IN_PLACE_APP, IN_PLACE_ARCHIVE } from "./constants.js";
import {
    inPlaceReleaseStore,
    inPlaceUserDirectory,
    readInPlaceInstallation,
} from "./in-place-receipt.js";
import { assertCompatibleExecutable, assertSystemPath } from "./in-place-target.js";
import { checkInPlaceUpgrade } from "./in-place-watcher.js";
import { verifyRelease } from "./release-manifest.js";
import { releasePointer } from "./releases.js";

function archiveDigest(file: string): string {
    const original = require("original-fs") as typeof fs;
    const digest = createHash("sha256");
    const handle = original.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    const buffer = Buffer.alloc(1024 * 1024);
    try {
        for (;;) {
            const count = original.readSync(handle, buffer, 0, buffer.length, null);
            if (!count) break;
            digest.update(buffer.subarray(0, count));
        }
    } finally {
        original.closeSync(handle);
    }
    return digest.digest("hex");
}

export function readRuntimeFile(): string {
    const store = inPlaceReleaseStore();
    assertSystemPath(path.join(store, "active.json"), false);
    const pointer = releasePointer(store);
    if (!pointer) throw new Error("缺少系统看板版本指针");
    const release = path.join(store, pointer.current);
    const manifest = path.join(release, "release.json");
    assertSystemPath(manifest, false);
    if (sha256File(manifest) !== pointer.current) throw new Error("系统看板版本指纹已变化");
    verifyRelease(release);
    const runtime = path.join(release, "dist/runtime/main.js");
    assertSystemPath(runtime, false);
    return runtime;
}

export function notifyInPlace(message: string): void {
    execFileSync("/usr/bin/notify-send", ["--app-name=Codex Enhancer", "Codex 会话看板", message], {
        stdio: "ignore",
        timeout: 8000,
    });
}

function showDisabled(version: string, error: unknown): void {
    const reason = error instanceof Error ? error.message : String(error);
    console.error("Codex Enhancer disabled", version, reason);
    const fingerprint = createHash("sha256").update(`${version}\n${reason}`).digest("hex");
    const message = `Codex ${version} 的会话看板已停用。${reason}。请安装兼容版看板或卸载原地安装。`;
    const electron = require("electron") as { app: App };
    void electron.app
        .whenReady()
        .then(() =>
            checkInPlaceUpgrade(
                inPlaceUserDirectory(),
                () => ({ kind: "incompatible", fingerprint, message }),
                () => notifyInPlace(message),
            ),
        )
        .catch((failure: unknown) => console.error("Codex Enhancer notification failed", failure));
}

export function startInPlace(): void {
    let version = "未知";
    try {
        if (
            process.platform !== "linux" ||
            process.arch !== "x64" ||
            fs.realpathSync(process.execPath) !== path.join(IN_PLACE_APP, "ChatGPT") ||
            process.resourcesPath !== path.join(IN_PLACE_APP, "resources")
        )
            throw new Error("当前窗口不是已登记的 Fedora Codex");
        const metadata = JSON.parse(
            fs.readFileSync(
                path.join(process.resourcesPath, "linux-package-metadata.json"),
                "utf8",
            ),
        ) as { version?: unknown };
        if (typeof metadata.version === "string") version = metadata.version;
        const installation = readInPlaceInstallation();
        if (installation?.phase !== "installed") throw new Error("安装记录缺失或尚未提交");
        if (installation.version !== version)
            throw new Error(`客户端已经替换已登记的 ${installation.version} 构建`);
        assertCompatibleExecutable(process.execPath);
        if (
            archiveDigest(path.join(IN_PLACE_APP, IN_PLACE_ARCHIVE)) !== installation.patched.sha256
        )
            throw new Error("客户端启动归档已变化");
        const electron = require("electron") as {
            app?: App;
            session?: { defaultSession?: unknown };
        };
        if (typeof electron.app?.whenReady !== "function" || !electron.session)
            throw new Error("Codex 当前窗口缺少所需的 Electron 接口");
        require(readRuntimeFile());
    } catch (error) {
        showDisabled(version, error);
    }
}
