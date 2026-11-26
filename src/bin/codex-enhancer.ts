#!/usr/bin/env node

import { dirname, resolve } from "node:path";
import { detect, findSystemApp, formatDetection } from "../installer/detect.js";
import { install } from "../installer/install.js";
import { launchAndConfirm } from "../installer/launch.js";
import { launchOfficial, officialStatus, stopOfficial } from "../installer/official/launch.js";
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

function officialFlags(rest: readonly string[]): Map<string, string> {
    const values = new Map<string, string>();
    for (let index = 0; index < rest.length; index += 2) {
        const key = rest[index];
        const value = rest[index + 1];
        if (!key?.startsWith("--") || value === undefined)
            throw new Error("官方 Codex 启动参数必须成对出现");
        if (!["--app", "--data-dir", "--profile"].includes(key))
            throw new Error(`未知的官方 Codex 启动参数：${key}`);
        values.set(key, value);
    }
    return values;
}

function officialLaunchOptions(rest: readonly string[]) {
    const values = officialFlags(rest);
    const profile = values.get("--profile");
    const appValue = values.get("--app");
    const app = appValue ? resolve(appValue) : undefined;
    const dataValue = values.get("--data-dir");
    const dataDir =
        (dataValue ? resolve(dataValue) : undefined) ?? (profile ? resolve(profile) : undefined);
    return {
        launch: {
            ...(app ? { appDir: app } : {}),
            ...(dataDir ? { dataDir } : {}),
            ...(profile ? { extraArgs: [`--user-data-dir=${resolve(profile)}`] } : {}),
        },
        argv: [
            ...(app ? ["--app", app] : []),
            ...(dataDir ? ["--data-dir", dataDir] : []),
            ...(profile ? ["--profile", resolve(profile)] : []),
        ],
    };
}

async function officialCheck(): Promise<object> {
    const { attachOfficial, evaluate, sidebarContext } = await import(
        "../installer/official/attach.js"
    );
    const { connection, port, page } = await attachOfficial();
    try {
        const window = await evaluate<{ ready: string; bridge: boolean }>(
            connection,
            `({ ready: document.readyState, bridge: typeof window.electronBridge === "object" })`,
        );
        const context = await sidebarContext(connection);
        return {
            port,
            page: { id: page.id, url: page.url },
            window,
            signedIn: Boolean(context.accountId && context.userId),
            sectionCount: context.sections.length,
            hosts: Object.keys(context.remoteHostNames ?? {}),
        };
    } finally {
        connection.close();
    }
}

function launcherOptions(rest: readonly string[]) {
    const index = rest.indexOf("--destination");
    if (index < 0) return { startArgs: officialLaunchOptions(rest).argv };
    const destination = rest[index + 1];
    if (!destination) throw new Error("--destination 需要一个应用路径");
    const remaining = [...rest.slice(0, index), ...rest.slice(index + 2)];
    return { destination, startArgs: officialLaunchOptions(remaining).argv };
}

async function runOfficial(args: readonly string[]): Promise<void> {
    if (process.platform !== "darwin") throw new Error("官方 Codex 启动器仅支持 macOS");
    const [action = "status", ...rest] = args;
    switch (action) {
        case "launch":
            console.log(
                JSON.stringify(await launchOfficial(officialLaunchOptions(rest).launch), null, 2),
            );
            return;
        case "start": {
            const launch = await launchOfficial(officialLaunchOptions(rest).launch);
            const { startOfficialBoard } = await import("../installer/official/board.js");
            const service = await startOfficialBoard(launch.pid);
            console.log(JSON.stringify({ launch, service }, null, 2));
            return;
        }
        case "serve": {
            const { serveOfficialBoard } = await import("../installer/official/board.js");
            await serveOfficialBoard();
            return;
        }
        case "install-launcher": {
            const { installLauncher } = await import("../installer/official/launcher.js");
            console.log(JSON.stringify(installLauncher(launcherOptions(rest)), null, 2));
            return;
        }
        case "stop": {
            const { stopOfficialBoard } = await import("../installer/official/board.js");
            const board = await stopOfficialBoard();
            const app = await stopOfficial();
            console.log(JSON.stringify({ stopped: { board, app } }));
            return;
        }
        case "status": {
            const { boardStatus } = await import("../installer/official/board.js");
            console.log(
                JSON.stringify(
                    { app: await officialStatus(), board: await boardStatus() },
                    null,
                    2,
                ),
            );
            return;
        }
        case "check":
            console.log(JSON.stringify(await officialCheck(), null, 2));
            return;
    }
    throw new Error(
        "用法：codex-enhancer official launch|start|serve|install-launcher|status|stop|check",
    );
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
} else if (command === "official") {
    void runOfficial(process.argv.slice(3)).catch(fail);
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
        "Usage: codex-enhancer detect|install [--in-place]|uninstall [--in-place]|purge-in-place-data|status [--in-place]|launch|official <launch|start|serve|install-launcher|status|stop|check>|update [--in-place] <directory>|rollback [--in-place]|package <version> <output> [platform]",
    );
    console.log(
        "       codex-enhancer archive-migration preview | archive-migration apply|rollback <token>",
    );
    process.exitCode = command ? 1 : 0;
}
