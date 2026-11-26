#!/usr/bin/env node

import { dirname } from "node:path";
import { detect, findSystemApp, formatDetection } from "../installer/detect.js";
import { install } from "../installer/install.js";
import { launchAndConfirm } from "../installer/launch.js";
import { outputDir, stateDir } from "../installer/paths.js";
import { releasePlatform, releasePlatforms } from "../installer/release/manifest.js";
import { packageRelease } from "../installer/release/package.js";
import { activateRelease, activeRelease, rollbackRelease } from "../installer/release/store.js";
import { restore } from "../installer/restore.js";
import { formatStatus, status } from "../installer/status.js";

const command = process.argv[2];

function fail(error: unknown): void {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
}

async function runInPlace(): Promise<void> {
    const installer = await import("../installer/in-place/index.js");
    const candidate = process.argv[4];
    const standalone = command === "in-place-check" || command === "purge-in-place-data";
    const expectedLength = standalone ? 3 : command === "update" ? 5 : 4;
    if (process.argv.length !== expectedLength) throw new Error("原地安装命令参数无效");
    if (command === "in-place-check")
        console.log(JSON.stringify(await installer.runInPlaceCheck()));
    else if (command === "purge-in-place-data")
        console.log(JSON.stringify({ removed: installer.purgeInPlaceData() }));
    else if (command === "install")
        console.log(JSON.stringify(installer.installInPlace(dirname(outputDir))));
    else if (command === "uninstall") console.log(JSON.stringify(installer.uninstallInPlace()));
    else if (command === "status") console.log(JSON.stringify(installer.inPlaceStatus()));
    else {
        if (command === "update" && !candidate) throw new Error("请提供已解压的新版看板目录");
        console.log(
            `看板版本 ${installer.adjustInPlaceRelease(candidate)}。请从原图标重启 Codex。`,
        );
    }
}

if (
    command === "in-place-check" ||
    command === "purge-in-place-data" ||
    (["install", "uninstall", "status", "update", "rollback"].includes(command ?? "") &&
        process.argv[3] === "--in-place")
) {
    void runInPlace().catch(fail);
} else if (command === "detect") {
    try {
        console.log(formatDetection(detect(findSystemApp().appDir)));
    } catch (error) {
        fail(error);
    }
} else if (command === "install") {
    try {
        const result = install();
        console.log(`action: ${result.action}`);
        console.log(`test copy: ${result.state.target.path}`);
        console.log(`codex version: ${result.state.app.codexVersion}`);
        console.log(`runtime: ${result.state.app.runtimeType}`);
        console.log(`entry: ${result.state.app.entry}`);
        console.log(`original sha256: ${result.state.files.archive.originalSha256}`);
        console.log(`patched sha256: ${result.state.files.archive.patchedSha256}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "uninstall") {
    try {
        const result = restore();
        console.log("action: removed");
        console.log(`archive sha256: ${result.archiveHash}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "status") {
    try {
        console.log(formatStatus(status()));
        const release = activeRelease();
        if (release) console.log(`board release: ${release.version}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "launch") {
    void launchAndConfirm(process.argv.slice(3)).then((pid) => console.log(`pid: ${pid}`), fail);
} else if (command === "task-migration" || command === "archive-migration") {
    void (async () => {
        const [action, confirm, extra] = process.argv.slice(3);
        if (
            extra ||
            !(action === "preview"
                ? !confirm
                : (action === "apply" ||
                      (command === "archive-migration" && action === "rollback")) &&
                  /^[\da-f]{64}$/.test(confirm ?? ""))
        )
            throw new Error(
                `用法：codex-enhancer ${command} preview 或 ${command} ${
                    command === "archive-migration" ? "apply|rollback" : "apply"
                } <迁移确认值>`,
            );
        const { connectBoard } = await import("../compat/client.js");
        const board = await connectBoard(stateDir);
        console.log(
            JSON.stringify(
                await board.request(
                    `/api/${command}/${action}`,
                    action === "preview" ? undefined : { confirm },
                ),
                null,
                2,
            ),
        );
    })().catch(fail);
} else if (command === "update" || command === "rollback") {
    try {
        const candidate = process.argv[3];
        if (command === "update" && !candidate)
            throw new Error("provide an extracted board package directory");
        const release =
            candidate && command === "update" ? activateRelease(candidate) : rollbackRelease();
        console.log(`board release: ${release.version}`);
        console.log("Restart the Enhancer copy to load this release.");
    } catch (error) {
        fail(error);
    }
} else if (command === "package") {
    try {
        const [version, directory, target] = process.argv.slice(3);
        if (!version || !directory) throw new Error("provide a version and output directory");
        const platform = target
            ? releasePlatforms.find((value) => value === target)
            : releasePlatform();
        if (!platform) throw new Error(`unsupported package target ${target}`);
        console.log(JSON.stringify(packageRelease(version, directory, platform)));
    } catch (error) {
        fail(error);
    }
} else {
    console.log(
        "Usage: codex-enhancer detect|install [--in-place]|uninstall [--in-place]|purge-in-place-data|status [--in-place]|launch|update [--in-place] <directory>|rollback [--in-place]|package <version> <output> [platform]",
    );
    console.log(
        "       codex-enhancer archive-migration preview | archive-migration apply|rollback <token>",
    );
    process.exitCode = command ? 1 : 0;
}
