import { execFileSync, spawn } from "node:child_process";
import {
    closeSync,
    existsSync,
    mkdirSync,
    openSync,
    readFileSync,
    rmSync,
    writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import * as path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { findSystemApp } from "../detect.js";
import { logs, stateDir } from "../paths.js";

export interface OfficialLaunch {
    appDir: string;
    binary: string;
    pid: number;
    dataDir: string;
    startedAt: string;
}

export interface OfficialStatus {
    launch: OfficialLaunch | null;
    running: boolean;
    port: number | null;
    target: boolean;
}

const directory = path.join(stateDir, "official");
const receiptPath = path.join(directory, "launch.json");
const logPath = path.join(logs, "official.log");
const targetUrl = "app://-/index.html";

export function defaultDataDir(): string {
    return path.join(homedir(), "Library", "Application Support", "Codex");
}

export function parsePort(value: string): number | null {
    const [line] = value.split("\n");
    const port = Number(line);
    return Number.isSafeInteger(port) && port > 0 && port < 65536 ? port : null;
}

export function readPort(dataDir: string): number | null {
    try {
        return parsePort(readFileSync(path.join(dataDir, "DevToolsActivePort"), "utf8"));
    } catch {
        return null;
    }
}

export function parseLaunch(value: string): OfficialLaunch | null {
    try {
        const launch = JSON.parse(value) as Partial<OfficialLaunch>;
        if (
            typeof launch.appDir !== "string" ||
            typeof launch.binary !== "string" ||
            typeof launch.pid !== "number" ||
            typeof launch.dataDir !== "string" ||
            typeof launch.startedAt !== "string"
        )
            return null;
        return launch as OfficialLaunch;
    } catch {
        return null;
    }
}

export function readLaunch(): OfficialLaunch | null {
    try {
        return parseLaunch(readFileSync(receiptPath, "utf8"));
    } catch {
        return null;
    }
}

export function isRunning(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch {
        return false;
    }
}

function writeLaunch(launch: OfficialLaunch): void {
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    writeFileSync(receiptPath, `${JSON.stringify(launch, null, 2)}\n`, { mode: 0o600 });
}

function clearLaunch(): void {
    rmSync(receiptPath, { force: true });
}

export async function targets(port: number, timeoutMs = 1200): Promise<unknown[]> {
    try {
        const response = await fetch(`http://127.0.0.1:${port}/json/list`, {
            signal: AbortSignal.timeout(timeoutMs),
        });
        if (!response.ok) return [];
        const value: unknown = await response.json();
        return Array.isArray(value) ? value : [];
    } catch {
        return [];
    }
}

interface DebugTarget {
    type?: unknown;
    id?: unknown;
    url?: unknown;
    webSocketDebuggerUrl?: unknown;
}

export function codexPage(
    value: unknown,
): { id: string; url: string; webSocketDebuggerUrl: string } | null {
    if (typeof value !== "object" || value === null) return null;
    const page = value as DebugTarget;
    if (page.type !== "page") return null;
    if (typeof page.id !== "string") return null;
    if (typeof page.url !== "string" || !page.url.startsWith(targetUrl)) return null;
    if (typeof page.webSocketDebuggerUrl !== "string") return null;
    return {
        id: page.id,
        url: page.url,
        webSocketDebuggerUrl: page.webSocketDebuggerUrl,
    };
}

export async function findCodexPage(port: number): Promise<ReturnType<typeof codexPage>> {
    for (const value of await targets(port)) {
        const page = codexPage(value);
        if (page) return page;
    }
    return null;
}

export function bundleBinary(appDir: string): string {
    const plistPath = path.join(appDir, "Contents", "Info.plist");
    const executable = existsSync(plistPath)
        ? /<key>CFBundleExecutable<\/key>\s*<string>([^<]+)<\/string>/.exec(
              readFileSync(plistPath, "utf8"),
          )?.[1]
        : undefined;
    return path.join(appDir, "Contents", "MacOS", executable ?? path.basename(appDir, ".app"));
}

function runningInstances(binary: string): number[] {
    const text = execFileSync("/bin/ps", ["-axo", "pid=,comm="], { encoding: "utf8" });
    const pids: number[] = [];
    for (const line of text.split("\n")) {
        const match = /^\s*(\d+)\s+(.+)$/.exec(line);
        if (match?.[2] === binary) pids.push(Number(match[1]));
    }
    return pids;
}

interface LaunchOptions {
    appDir?: string;
    dataDir?: string;
    extraArgs?: readonly string[];
    timeoutMs?: number;
}

function locateLaunch(options: LaunchOptions): { appDir: string; binary: string; dataDir: string } {
    const system = options.appDir ? null : findSystemApp();
    const locatedApp = options.appDir ?? system?.appDir;
    if (!locatedApp) throw new Error("找不到官方 Codex 应用");
    const appDir = path.resolve(locatedApp);
    const binary = path.resolve(bundleBinary(appDir));
    if (!existsSync(binary)) throw new Error(`官方 Codex 启动文件不存在：${binary}`);
    const dataDir = path.resolve(options.dataDir ?? defaultDataDir());
    mkdirSync(dataDir, { recursive: true, mode: 0o700 });
    return { appDir, binary, dataDir };
}

async function reusableLaunch(
    appDir: string,
    binary: string,
    dataDir: string,
): Promise<OfficialLaunch | null> {
    const previous = readLaunch();
    if (
        previous &&
        isRunning(previous.pid) &&
        path.resolve(previous.appDir) === appDir &&
        path.resolve(previous.dataDir) === dataDir
    ) {
        const port = readPort(previous.dataDir);
        if (port && (await findCodexPage(port))) return previous;
    }
    if (runningInstances(binary).length > 0)
        throw new Error("Codex 已在运行但没有调试端口，请先完全退出 Codex 再启动");
    return null;
}

function startCodex(binary: string, extraArgs: readonly string[]): number {
    mkdirSync(logs, { recursive: true, mode: 0o700 });
    const logFd = openSync(logPath, "a", 0o600);
    const child = spawn(
        binary,
        ["--remote-debugging-port=0", "--remote-debugging-address=127.0.0.1", ...extraArgs],
        {
            cwd: homedir(),
            env: process.env,
            detached: true,
            stdio: ["ignore", logFd, logFd],
        },
    );
    child.unref();
    closeSync(logFd);
    if (child.pid === undefined) throw new Error("无法启动 Codex");
    return child.pid;
}

async function waitForEndpoint(dataDir: string, pid: number, timeoutMs: number): Promise<boolean> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        const port = readPort(dataDir);
        if (port && (await findCodexPage(port))) return true;
        if (!isRunning(pid)) return false;
        await delay(250);
    }
    return false;
}

export async function launchOfficial(options: LaunchOptions): Promise<OfficialLaunch> {
    const { appDir, binary, dataDir } = locateLaunch(options);
    const previous = await reusableLaunch(appDir, binary, dataDir);
    if (previous) return previous;
    const pid = startCodex(binary, options.extraArgs ?? []);
    if (!(await waitForEndpoint(dataDir, pid, options.timeoutMs ?? 25000))) {
        if (isRunning(pid)) process.kill(pid, "SIGTERM");
        clearLaunch();
        throw new Error(`Codex 未在限定时间内提供看板入口，请查看日志：${logPath}`);
    }
    const launch = { appDir, binary, pid, dataDir, startedAt: new Date().toISOString() };
    writeLaunch(launch);
    return launch;
}

export async function officialStatus(): Promise<OfficialStatus> {
    const launch = readLaunch();
    if (!launch) return { launch: null, running: false, port: null, target: false };
    const running = isRunning(launch.pid);
    const port = running ? readPort(launch.dataDir) : null;
    const target = port !== null && (await findCodexPage(port)) !== null;
    return { launch, running, port, target };
}

export async function stopOfficial(): Promise<boolean> {
    const launch = readLaunch();
    if (!launch) return false;
    if (isRunning(launch.pid)) {
        process.kill(launch.pid, "SIGTERM");
        const deadline = Date.now() + 5000;
        while (Date.now() < deadline && isRunning(launch.pid)) await delay(200);
        if (isRunning(launch.pid)) process.kill(launch.pid, "SIGKILL");
    }
    clearLaunch();
    return true;
}
