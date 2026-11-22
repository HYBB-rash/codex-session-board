import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { listAsarFiles, lookup, openArchive, readEntry } from "./asar.js";

const FUSE_SENTINEL = "dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX";
const FUSE_NAMES = [
    "RunAsNode",
    "EnableCookieEncryption",
    "EnableNodeOptionsEnvironmentVariable",
    "EnableNodeCliInspectArguments",
    "EnableEmbeddedAsarIntegrityValidation",
    "OnlyLoadAppFromAsar",
    "LoadBrowserProcessSpecificV8Snapshot",
    "GrantFileProtocolExtraPrivileges",
    "WasmTrapHandlers",
];

export interface FuseInfo {
    version: number;
    states: string;
    flags: Record<string, boolean>;
}

export interface StartupArchive {
    archivePath: string;
    relativeArchive: string;
    main: string;
    packageName: string | null;
    packageVersion: string | null;
}

export interface SystemApp {
    appDir: string;
    startScript: string;
    binary: string;
}

export interface Detection {
    appDir: string;
    startScript: string;
    binary: string;
    codexVersion: string;
    buildFlavor: string | null;
    chromiumVersion: string;
    runtimeType: string | null;
    runtimeArchiveSha: string | null;
    rpm: string | null;
    fuses: FuseInfo | null;
    archive: StartupArchive;
}

function readText(filePath: string): string {
    return fs.readFileSync(filePath, "utf8").trim();
}

function readJson(filePath: string): unknown {
    return JSON.parse(fs.readFileSync(filePath, "utf8")) as unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function field(value: unknown, key: string): unknown {
    if (!isRecord(value) || !(key in value)) return undefined;
    return value[key];
}

function requiredString(value: unknown, key: string, filePath: string): string {
    const item = field(value, key);
    if (typeof item !== "string") throw new Error(`${key} is missing in ${filePath}`);
    return item;
}

function optionalString(value: unknown, key: string): string | null {
    const item = field(value, key);
    if (typeof item !== "string") return null;
    return item;
}

function commandOutput(command: string, args: string[]): string | null {
    try {
        return execFileSync(command, args, { encoding: "utf8" }).trim();
    } catch {
        return null;
    }
}

function resolveLauncher(launcherPath: string): string {
    const resolved = fs.realpathSync(launcherPath);
    const text = fs.readFileSync(resolved, "utf8");
    const match = text.match(/exec\s+(\S*start\.sh)/);
    const startScript = match?.[1];
    if (startScript) return path.resolve(path.dirname(resolved), startScript);
    if (resolved.endsWith("start.sh")) return resolved;
    throw new Error(`could not resolve a start.sh from ${launcherPath}`);
}

export function findSystemApp(): SystemApp {
    const candidates: string[] = [];
    if (fs.existsSync("/usr/bin/codex-desktop")) candidates.push("/usr/bin/codex-desktop");
    if (fs.existsSync("/opt/codex-desktop/start.sh"))
        candidates.push("/opt/codex-desktop/start.sh");
    const launcher = candidates[0];
    if (launcher === undefined) throw new Error("Codex Desktop launcher was not found");
    const startScript = resolveLauncher(launcher);
    const appDir = path.dirname(startScript);
    const binary = path.join(appDir, "ChatGPT");
    if (!fs.existsSync(binary)) throw new Error(`ChatGPT binary is missing in ${appDir}`);
    return { appDir, startScript, binary };
}

export function readFuses(binaryPath: string): FuseInfo | null {
    const fd = fs.openSync(binaryPath, "r");
    try {
        const size = fs.fstatSync(fd).size;
        const sentinel = Buffer.from(FUSE_SENTINEL);
        const chunkSize = 1024 * 1024;
        for (let offset = 0; offset < size; offset += chunkSize - sentinel.length) {
            const length = Math.min(chunkSize, size - offset);
            const chunk = Buffer.alloc(length);
            fs.readSync(fd, chunk, 0, length, offset);
            const index = chunk.indexOf(sentinel);
            if (index < 0) continue;
            const slice = Buffer.alloc(64);
            fs.readSync(fd, slice, 0, slice.length, offset + index);
            const version = slice[sentinel.length];
            const count = slice[sentinel.length + 1];
            if (version === undefined || count === undefined) return null;
            const states = slice
                .subarray(sentinel.length + 2, sentinel.length + 2 + count)
                .toString("ascii");
            const pairs: Array<[string, boolean]> = [...states].map((state, fuseIndex) => [
                FUSE_NAMES[fuseIndex] ?? `fuse${fuseIndex}`,
                state === "1",
            ]);
            return { version, states, flags: Object.fromEntries(pairs) };
        }
    } finally {
        fs.closeSync(fd);
    }
    return null;
}

export function findStartupArchive(appDir: string): StartupArchive {
    const resourcesDir = path.join(appDir, "resources");
    const names = listAsarFiles(resourcesDir);
    const matches: StartupArchive[] = [];
    for (const name of names) {
        const archivePath = path.join(resourcesDir, name);
        const archive = openArchive(archivePath);
        try {
            if (!lookup(archive.header, "package.json")) continue;
            const manifest: unknown = JSON.parse(
                readEntry(archive, "package.json").toString("utf8"),
            );
            const main = field(manifest, "main");
            if (typeof main !== "string" || !lookup(archive.header, main)) continue;
            const packageName = field(manifest, "name");
            const packageVersion = field(manifest, "version");
            matches.push({
                archivePath,
                relativeArchive: path.posix.join("resources", name),
                main,
                packageName: typeof packageName === "string" ? packageName : null,
                packageVersion: typeof packageVersion === "string" ? packageVersion : null,
            });
        } finally {
            fs.closeSync(archive.fd);
        }
    }
    const startup = matches[0];
    if (matches.length !== 1 || startup === undefined) {
        throw new Error(`expected one app archive with a main entry, found ${matches.length}`);
    }
    return startup;
}

export function detect(appDir: string = findSystemApp().appDir): Detection {
    const located: SystemApp =
        appDir === findSystemApp().appDir
            ? findSystemApp()
            : {
                  appDir,
                  startScript: path.join(appDir, "start.sh"),
                  binary: path.join(appDir, "ChatGPT"),
              };
    const metadataPath = path.join(located.appDir, "resources", "linux-package-metadata.json");
    const owlPath = path.join(located.appDir, "resources", "owl-electron-app.json");
    const versionPath = path.join(located.appDir, "version");
    if (!fs.existsSync(metadataPath) || !fs.existsSync(owlPath) || !fs.existsSync(versionPath)) {
        throw new Error(`Codex metadata is incomplete in ${located.appDir}`);
    }
    const metadata = readJson(metadataPath);
    const owl = readJson(owlPath);
    const startup = findStartupArchive(located.appDir);
    return {
        appDir: located.appDir,
        startScript: located.startScript,
        binary: located.binary,
        codexVersion: requiredString(metadata, "version", metadataPath),
        buildFlavor: optionalString(metadata, "codexBuildFlavor"),
        chromiumVersion: readText(versionPath),
        runtimeType: optionalString(owl, "runtimeName"),
        runtimeArchiveSha: optionalString(owl, "runtimeArchiveSha"),
        rpm: commandOutput("rpm", ["-q", "codex-desktop"]),
        fuses: readFuses(located.binary),
        archive: startup,
    };
}

export function formatDetection(info: Detection): string {
    const fuseLine = info.fuses
        ? Object.entries(info.fuses.flags)
              .map(([name, enabled]) => `${name}=${enabled ? "on" : "off"}`)
              .join(", ")
        : "not found";
    return [
        `path: ${info.appDir}`,
        `codex version: ${info.codexVersion}`,
        `runtime: ${info.runtimeType}`,
        `chromium: ${info.chromiumVersion}`,
        `package: ${info.rpm ?? "not an rpm"}`,
        `archive: ${info.archive.relativeArchive}`,
        `entry: ${info.archive.main}`,
        `fuses: ${fuseLine}`,
    ].join("\n");
}
