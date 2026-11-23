import { type ChildProcess, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { APP_ID } from "./constants.js";
import { assertTestCopyStopped } from "./copy.js";
import { locateApp } from "./detect.js";
import {
    type InstallState,
    readState,
    restoreEmbeddedMac,
    withRecoveredMacCopy,
    writeState,
} from "./install.js";
import * as paths from "./paths.js";

function deploymentId(): string {
    return createHash("sha256").update(paths.stateDir).digest("hex").slice(0, 12);
}

function bridgeSocket(appId: string): string {
    const override = process.env.CODEX_ENHANCER_BRIDGE_SOCKET;
    if (override) return path.resolve(override);
    const runtime = process.env.XDG_RUNTIME_DIR || `/run/user/${process.getuid?.() ?? 0}`;
    const directory = path.join(runtime, appId, "app-server-bridge");
    fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
    return path.join(directory, "app-server.sock");
}

function launchEnvironment(resourcesDir: string): NodeJS.ProcessEnv {
    const env: NodeJS.ProcessEnv = {
        ...process.env,
        CODEX_HOME: paths.codexHome,
        CODEX_ENHANCER_STATE_DIR: paths.stateDir,
        CODEX_ELECTRON_USER_DATA_PATH: paths.userData,
        CODEX_SPARKLE_ENABLED: "false",
        CODEX_ENHANCER_AUTO_OPEN: process.env.CODEX_ENHANCER_AUTO_OPEN ?? "0",
    };
    const cli = process.env.CODEX_CLI_PATH;
    if (cli) env.CODEX_CLI_PATH = path.resolve(cli);
    if (process.platform === "darwin") return env;
    const appId = `${APP_ID}-${deploymentId()}`;
    return {
        ...env,
        CODEX_LINUX_APP_ID: appId,
        CODEX_LINUX_APP_DISPLAY_NAME: "Codex Enhancer",
        CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET: bridgeSocket(appId),
        CODEX_CLI_PATH: env.CODEX_CLI_PATH || path.join(resourcesDir, "codex"),
        XDG_CONFIG_HOME: paths.xdgConfig,
        CODEX_LINUX_DISABLE_USAGE_REPORTING: "1",
    };
}

const desktopLog = path.join(paths.logs, "desktop.log");
const runtimeLog = path.join(paths.logs, "runtime.jsonl");

function launchCopy(state: InstallState | null, extraArgs: readonly string[]): ChildProcess {
    if (state?.status !== "installed") {
        throw new Error("install the test copy before launching it");
    }
    if (
        extraArgs.some(
            (argument) => argument === "--user-data-dir" || argument.startsWith("--user-data-dir="),
        )
    ) {
        throw new Error("the Enhancer user data directory is managed by its launcher");
    }
    const { resourcesDir, startScript } = locateApp(state.target.path);
    if (!fs.existsSync(startScript)) throw new Error(`test launcher is missing: ${startScript}`);
    fs.mkdirSync(paths.userData, { recursive: true });
    fs.mkdirSync(paths.codexHome, { recursive: true });
    fs.mkdirSync(paths.xdgConfig, { recursive: true });
    fs.mkdirSync(paths.logs, { recursive: true });
    const captureOutput =
        (state.mac && !state.mac.bootedAt) || process.env.CODEX_ENHANCER_LOG_STDOUT === "1";
    const logFd = captureOutput ? fs.openSync(desktopLog, "a", 0o600) : null;
    const child = spawn(startScript, [`--user-data-dir=${paths.userData}`, ...extraArgs], {
        cwd: state.target.path,
        detached: true,
        stdio: logFd == null ? ["ignore", "inherit", "inherit"] : ["ignore", logFd, logFd],
        env: launchEnvironment(resourcesDir),
    });
    child.unref();
    if (logFd != null) fs.closeSync(logFd);
    return child;
}

function didLoadWindow(start: number): boolean {
    if (!fs.existsSync(runtimeLog)) return false;
    const file = fs.openSync(runtimeLog, "r");
    try {
        const end = fs.fstatSync(file).size;
        const offset = Math.max(start, end - 256 * 1024);
        const bytes = Buffer.alloc(Math.max(0, end - offset));
        fs.readSync(file, bytes, 0, bytes.length, offset);
        return /"event":"did-finish-load","id":\d+,"url":"app:\/\//.test(bytes.toString("utf8"));
    } finally {
        fs.closeSync(file);
    }
}

function matchingFirstBoot(
    previous: InstallState | null,
    started: InstallState,
): InstallState | null {
    if (
        previous?.status !== "installed" ||
        previous.installedAt !== started.installedAt ||
        !previous.mac ||
        previous.mac.bootedAt
    )
        return null;
    return previous;
}

async function restoreFailedBoot(state: InstallState): Promise<void> {
    for (let attempt = 0; ; attempt += 1) {
        try {
            assertTestCopyStopped();
            break;
        } catch (error) {
            if (attempt >= 24) throw error;
            await delay(250);
        }
    }
    withRecoveredMacCopy((previous) => {
        const current = matchingFirstBoot(previous, state);
        if (!current) return;
        assertTestCopyStopped();
        restoreEmbeddedMac(current);
    });
}

async function confirmFirstBoot(
    child: ChildProcess,
    state: InstallState,
    offset: number,
): Promise<void> {
    let spawnError: Error | undefined;
    child.once("error", (error) => {
        spawnError = error;
    });
    const deadline = Date.now() + 45000;
    while (Date.now() < deadline) {
        if (spawnError || child.exitCode !== null || child.signalCode !== null) {
            const message = `the macOS copy exited during first launch; see ${desktopLog}`;
            try {
                await restoreFailedBoot(state);
            } catch (error) {
                throw new Error(
                    `${message}; restoring the official bundle failed: ${String(error)}`,
                    {
                        cause: error,
                    },
                );
            }
            throw new Error(message, { cause: spawnError });
        }
        if (didLoadWindow(offset)) {
            withRecoveredMacCopy((previous) => {
                const current = matchingFirstBoot(previous, state);
                if (current?.mac) {
                    current.mac.bootedAt = new Date().toISOString();
                    writeState(current);
                }
            });
            return;
        }
        await delay(250);
    }
    throw new Error(`the macOS copy is still starting; see ${desktopLog}`);
}

export function launch(extraArgs: readonly string[] = []): number {
    const child = launchCopy(readState(), extraArgs);
    if (child.pid === undefined) {
        child.once("error", () => {});
        throw new Error("test copy did not start");
    }
    return child.pid;
}

export async function launchAndConfirm(extraArgs: readonly string[] = []): Promise<number> {
    if (process.platform !== "darwin" || !readState()?.mac) return launch(extraArgs);
    const start = (state: InstallState | null) => {
        if (state?.mac && !state.mac.bootedAt) {
            assertTestCopyStopped();
            const offset = fs.statSync(runtimeLog, { throwIfNoEntry: false })?.size ?? 0;
            return { child: launchCopy(state, extraArgs), firstBoot: { state, offset } };
        }
        return { child: launchCopy(state, extraArgs), firstBoot: null };
    };
    const { child, firstBoot } = withRecoveredMacCopy(start);
    if (firstBoot) await confirmFirstBoot(child, firstBoot.state, firstBoot.offset);
    else if (child.pid === undefined) {
        await new Promise<never>((_, reject) => child.once("error", reject));
    }
    if (child.pid === undefined) throw new Error("test copy did not start");
    return child.pid;
}
