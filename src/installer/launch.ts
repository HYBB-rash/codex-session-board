import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { APP_ID } from "./constants.js";
import { locateApp } from "./detect.js";
import { readState } from "./install.js";
import * as paths from "./paths.js";

function deploymentId(): string {
    return createHash("sha256").update(paths.stateDir).digest("hex").slice(0, 12);
}

function bridgeSocket(appId: string): string {
    const override = process.env.CODEX_ENHANCER_BRIDGE_SOCKET;
    if (override) return override;
    const runtime = process.env.XDG_RUNTIME_DIR ?? `/run/user/${process.getuid?.() ?? 0}`;
    const directory = path.join(runtime, appId, "app-server-bridge");
    fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
    return path.join(directory, "app-server.sock");
}

function launchEnvironment(): NodeJS.ProcessEnv {
    const env = {
        ...process.env,
        CODEX_HOME: paths.codexHome,
        CODEX_ENHANCER_STATE_DIR: paths.stateDir,
        CODEX_ELECTRON_USER_DATA_PATH: paths.userData,
        CODEX_SPARKLE_ENABLED: "false",
        CODEX_ENHANCER_AUTO_OPEN: process.env.CODEX_ENHANCER_AUTO_OPEN ?? "0",
    };
    if (process.platform === "darwin") return env;
    const appId = `${APP_ID}-${deploymentId()}`;
    return {
        ...env,
        CODEX_LINUX_APP_ID: appId,
        CODEX_LINUX_APP_DISPLAY_NAME: "Codex Enhancer",
        CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET: bridgeSocket(appId),
        CODEX_CLI_PATH: path.join(paths.testApp, "resources", "codex"),
        XDG_CONFIG_HOME: paths.xdgConfig,
        CODEX_LINUX_DISABLE_USAGE_REPORTING: "1",
    };
}

export function launch(extraArgs: readonly string[] = []): number {
    const state = readState();
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
    const { startScript } = locateApp(state.target.path);
    if (!fs.existsSync(startScript)) throw new Error(`test launcher is missing: ${startScript}`);
    fs.mkdirSync(paths.userData, { recursive: true });
    fs.mkdirSync(paths.codexHome, { recursive: true });
    fs.mkdirSync(paths.xdgConfig, { recursive: true });
    fs.mkdirSync(paths.logs, { recursive: true });
    const desktopLog = path.join(paths.logs, "desktop.log");
    const logFd =
        process.env.CODEX_ENHANCER_LOG_STDOUT === "1" ? fs.openSync(desktopLog, "a") : null;
    const child = spawn(startScript, [`--user-data-dir=${paths.userData}`, ...extraArgs], {
        cwd: state.target.path,
        detached: true,
        stdio: logFd == null ? ["ignore", "inherit", "inherit"] : ["ignore", logFd, logFd],
        env: launchEnvironment(),
    });
    child.unref();
    if (logFd != null) fs.closeSync(logFd);
    if (child.pid === undefined) throw new Error("test copy did not start");
    return child.pid;
}
