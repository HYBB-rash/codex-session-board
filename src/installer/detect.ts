import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import { homedir } from "node:os";
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
    resourcesDir: string;
}

export interface Detection {
    appDir: string;
    platform: "linux" | "darwin";
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

interface PackageMetadata {
    version: unknown;
    codexBuildFlavor?: unknown;
}

interface OwlApp {
    runtimeName?: unknown;
    runtimeArchiveSha?: unknown;
}

interface PackageManifest {
    main?: unknown;
    name?: unknown;
    version?: unknown;
}

function isFields<T extends object>(value: unknown): value is T {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requiredString(value: unknown, name: string, filePath: string): string {
    if (typeof value !== "string") throw new Error(`${name} is missing in ${filePath}`);
    return value;
}

function optionalString(value: unknown): string | null {
    return typeof value === "string" ? value : null;
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
    const selected = process.env.CODEX_ENHANCER_SOURCE;
    if (selected) return locateApp(fs.realpathSync(selected));
    if (process.platform === "darwin") return findMacApp();
    if (process.platform !== "linux") throw new Error("unsupported Codex Desktop platform");
    const candidates: string[] = [];
    if (fs.existsSync("/usr/bin/codex-desktop")) candidates.push("/usr/bin/codex-desktop");
    if (fs.existsSync("/opt/codex-desktop/start.sh"))
        candidates.push("/opt/codex-desktop/start.sh");
    const launcher = candidates[0];
    if (launcher === undefined) throw new Error("Codex Desktop launcher was not found");
    const startScript = resolveLauncher(launcher);
    return locateApp(fs.realpathSync(path.dirname(startScript)));
}

function findMacApp(): SystemApp {
    const products = ["Codex.app", "ChatGPT.app"];
    const locations = ["/Applications", path.join(homedir(), "Applications")];
    for (const location of locations) {
        for (const product of products) {
            const directory = path.join(location, product);
            if (fs.existsSync(path.join(directory, "Contents", "Resources", "app.asar"))) {
                return locateApp(fs.realpathSync(directory));
            }
        }
    }
    throw new Error("Codex Desktop application was not found in Applications");
}

function plistString(info: string, key: string): string {
    return execFileSync("/usr/bin/plutil", ["-extract", key, "raw", "-o", "-", info], {
        encoding: "utf8",
    }).trim();
}

export function locateApp(appDir: string): SystemApp {
    if (process.platform === "darwin" || appDir.endsWith(".app")) {
        const contents = path.join(appDir, "Contents");
        const name = plistString(path.join(contents, "Info.plist"), "CFBundleExecutable");
        if (!name || name !== path.basename(name) || name === "..")
            throw new Error("invalid app bundle executable");
        const binary = path.join(contents, "MacOS", name);
        return {
            appDir,
            startScript: binary,
            binary,
            resourcesDir: path.join(contents, "Resources"),
        };
    }
    return {
        appDir,
        startScript: path.join(appDir, "start.sh"),
        binary: path.join(appDir, "ChatGPT"),
        resourcesDir: path.join(appDir, "resources"),
    };
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

export function findStartupArchive(
    appDir: string,
    resourcesDir = locateApp(appDir).resourcesDir,
): StartupArchive {
    const names = listAsarFiles(resourcesDir);
    const matches: StartupArchive[] = [];
    for (const name of names) {
        const archivePath = path.join(resourcesDir, name);
        const archive = openArchive(archivePath);
        try {
            if (!lookup(archive.header, "package.json")) continue;
            const parsed: unknown = JSON.parse(readEntry(archive, "package.json").toString("utf8"));
            const manifest = isFields<PackageManifest>(parsed) ? parsed : null;
            const main = manifest?.main;
            if (typeof main !== "string" || !lookup(archive.header, main)) continue;
            const packageName = manifest?.name;
            const packageVersion = manifest?.version;
            matches.push({
                archivePath,
                relativeArchive: path.relative(appDir, archivePath),
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
    const located = locateApp(appDir);
    if (process.platform === "darwin" || appDir.endsWith(".app")) return detectMac(located);
    const metadataPath = path.join(located.appDir, "resources", "linux-package-metadata.json");
    const owlPath = path.join(located.appDir, "resources", "owl-electron-app.json");
    const versionPath = path.join(located.appDir, "version");
    if (!fs.existsSync(metadataPath) || !fs.existsSync(owlPath) || !fs.existsSync(versionPath)) {
        throw new Error(`Codex metadata is incomplete in ${located.appDir}`);
    }
    const metadataJson = readJson(metadataPath);
    const metadata = isFields<PackageMetadata>(metadataJson) ? metadataJson : null;
    const owlJson = readJson(owlPath);
    const owl = isFields<OwlApp>(owlJson) ? owlJson : null;
    const startup = findStartupArchive(located.appDir);
    return {
        appDir: located.appDir,
        platform: "linux",
        startScript: located.startScript,
        binary: located.binary,
        codexVersion: requiredString(metadata?.version, "version", metadataPath),
        buildFlavor: optionalString(metadata?.codexBuildFlavor),
        chromiumVersion: readText(versionPath),
        runtimeType: optionalString(owl?.runtimeName),
        runtimeArchiveSha: optionalString(owl?.runtimeArchiveSha),
        rpm: commandOutput("rpm", ["-q", "codex-desktop"]),
        fuses: readFuses(located.binary),
        archive: startup,
    };
}

function detectMac(located: SystemApp): Detection {
    const archive = findStartupArchive(located.appDir, located.resourcesDir);
    const owl = path.join(located.resourcesDir, "owl-electron-app.json");
    const metadata = fs.existsSync(owl) ? (readJson(owl) as OwlApp) : null;
    return {
        appDir: located.appDir,
        platform: "darwin",
        startScript: located.startScript,
        binary: located.binary,
        codexVersion: plistString(
            path.join(located.appDir, "Contents", "Info.plist"),
            "CFBundleShortVersionString",
        ),
        buildFlavor: null,
        chromiumVersion: "unknown",
        runtimeType: optionalString(metadata?.runtimeName) ?? "electron",
        runtimeArchiveSha: optionalString(metadata?.runtimeArchiveSha),
        rpm: null,
        fuses: readFuses(located.binary),
        archive,
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
