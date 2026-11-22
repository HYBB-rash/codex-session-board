import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { entryIncludes, openArchive, patchEntry, readEntry, sha256File } from "./asar.js";
import { LINK_NAME, loaderStub, PATCH_VERSION, SENTINEL } from "./constants.js";
import { type Detection, detect, findSystemApp } from "./detect.js";
import * as paths from "./paths.js";

export interface InstallState {
    schema: 1;
    status: "installed" | "removed";
    patchVersion: string;
    installedAt: string;
    removedAt?: string;
    source: {
        path: string;
        codexVersion: string;
        runtimeType: string | null;
        chromiumVersion: string;
        rpm: string | null;
    };
    target: {
        kind: "test-copy";
        path: string;
    };
    app: {
        codexVersion: string;
        runtimeType: string | null;
        chromiumVersion: string;
        archive: string;
        entry: string;
    };
    files: {
        archive: {
            path: string;
            originalSha256: string;
            patchedSha256: string;
        };
    };
    backup: {
        archive: string;
    };
    loader: {
        sentinel: string;
        link: string;
        runtime: string;
    };
}

export interface InstallResult {
    action: "unchanged" | "patched" | "already-patched";
    state: InstallState;
    source: Detection;
    copy: Detection;
}

interface RawSource {
    path: unknown;
    codexVersion: unknown;
    runtimeType: unknown;
    chromiumVersion: unknown;
    rpm: unknown;
}

interface RawTarget {
    kind: unknown;
    path: unknown;
}

interface RawApp {
    codexVersion: unknown;
    runtimeType: unknown;
    chromiumVersion: unknown;
    archive: unknown;
    entry: unknown;
}

interface RawArchive {
    path: unknown;
    originalSha256: unknown;
    patchedSha256: unknown;
}

interface RawFiles {
    archive: unknown;
}

interface RawBackup {
    archive: unknown;
}

interface RawLoader {
    sentinel: unknown;
    link: unknown;
    runtime: unknown;
}

interface RawInstallState {
    schema: unknown;
    status: unknown;
    removedAt?: unknown;
    patchVersion: unknown;
    installedAt: unknown;
    source: unknown;
    target: unknown;
    app: unknown;
    files: unknown;
    backup: unknown;
    loader: unknown;
}

function isFields<T extends object>(value: unknown): value is T {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function expectFields<T extends object>(value: unknown, label: string): T {
    if (!isFields<T>(value)) throw new Error(`install state ${label} is not an object`);
    return value;
}

function expectString(value: unknown, label: string): string {
    if (typeof value !== "string") throw new Error(`install state ${label} is not a string`);
    return value;
}

function expectNullableString(value: unknown, label: string): string | null {
    if (value === null) return null;
    if (typeof value !== "string") throw new Error(`install state ${label} is not a string`);
    return value;
}

function readSource(value: unknown): InstallState["source"] {
    const source = expectFields<RawSource>(value, "source");
    return {
        path: expectString(source.path, "source.path"),
        codexVersion: expectString(source.codexVersion, "source.codexVersion"),
        runtimeType: expectNullableString(source.runtimeType, "source.runtimeType"),
        chromiumVersion: expectString(source.chromiumVersion, "source.chromiumVersion"),
        rpm: expectNullableString(source.rpm, "source.rpm"),
    };
}

function readApp(value: unknown): InstallState["app"] {
    const app = expectFields<RawApp>(value, "app");
    return {
        codexVersion: expectString(app.codexVersion, "app.codexVersion"),
        runtimeType: expectNullableString(app.runtimeType, "app.runtimeType"),
        chromiumVersion: expectString(app.chromiumVersion, "app.chromiumVersion"),
        archive: expectString(app.archive, "app.archive"),
        entry: expectString(app.entry, "app.entry"),
    };
}

function readArchive(value: unknown): InstallState["files"]["archive"] {
    const archive = expectFields<RawArchive>(value, "files.archive");
    return {
        path: expectString(archive.path, "files.archive.path"),
        originalSha256: expectString(archive.originalSha256, "files.archive.originalSha256"),
        patchedSha256: expectString(archive.patchedSha256, "files.archive.patchedSha256"),
    };
}

function parseState(value: unknown): InstallState {
    if (!isFields<RawInstallState>(value)) throw new Error("install state is not an object");
    if (value.schema !== 1) throw new Error("install state schema is not 1");
    if (value.status !== "installed" && value.status !== "removed") {
        throw new Error("install state status is not installed or removed");
    }
    if (value.removedAt !== undefined && typeof value.removedAt !== "string") {
        throw new Error("install state removedAt is not a string");
    }
    const target = expectFields<RawTarget>(value.target, "target");
    if (target.kind !== "test-copy") throw new Error("install state target.kind is not test-copy");
    const files = expectFields<RawFiles>(value.files, "files");
    const backup = expectFields<RawBackup>(value.backup, "backup");
    const loader = expectFields<RawLoader>(value.loader, "loader");
    const state: InstallState = {
        schema: 1,
        status: value.status,
        patchVersion: expectString(value.patchVersion, "patchVersion"),
        installedAt: expectString(value.installedAt, "installedAt"),
        source: readSource(value.source),
        target: { kind: "test-copy", path: expectString(target.path, "target.path") },
        app: readApp(value.app),
        files: { archive: readArchive(files.archive) },
        backup: { archive: expectString(backup.archive, "backup.archive") },
        loader: {
            sentinel: expectString(loader.sentinel, "loader.sentinel"),
            link: expectString(loader.link, "loader.link"),
            runtime: expectString(loader.runtime, "loader.runtime"),
        },
    };
    if (typeof value.removedAt === "string") state.removedAt = value.removedAt;
    return state;
}

export function readState(): InstallState | null {
    if (!fs.existsSync(paths.installState)) return null;
    return parseState(JSON.parse(fs.readFileSync(paths.installState, "utf8")));
}

export function writeState(state: InstallState): void {
    fs.mkdirSync(paths.stateDir, { recursive: true });
    const next = `${paths.installState}.next`;
    fs.writeFileSync(next, `${JSON.stringify(state, null, 2)}\n`);
    fs.renameSync(next, paths.installState);
}

function assertCopyIsIsolated(copyDir: string, systemDir: string): void {
    const copy = path.resolve(copyDir);
    const system = path.resolve(systemDir);
    if (copy === system || copy.startsWith(`${system}${path.sep}`)) {
        throw new Error(`refusing to modify the installed Codex at ${system}`);
    }
}

function ensureTestCopy(systemDir: string): boolean {
    assertCopyIsIsolated(paths.testApp, systemDir);
    if (
        fs.existsSync(path.join(paths.testApp, "ChatGPT")) &&
        fs.existsSync(path.join(paths.testApp, "start.sh"))
    ) {
        return false;
    }
    fs.rmSync(paths.testApp, { recursive: true, force: true });
    fs.mkdirSync(paths.stateDir, { recursive: true });
    execFileSync("cp", ["-a", "--reflink=auto", systemDir, paths.testApp], { stdio: "inherit" });
    return true;
}

function backupBaseline(archivePath: string, version: string, currentHash: string): string {
    const directory = path.join(paths.backupDir, version);
    fs.mkdirSync(directory, { recursive: true });
    const stable = path.join(directory, "app.asar");
    if (!fs.existsSync(stable)) {
        execFileSync("cp", ["-a", "--reflink=auto", archivePath, stable], { stdio: "inherit" });
        return stable;
    }
    if (sha256File(stable) === currentHash) return stable;
    const versioned = path.join(directory, `${currentHash}.asar`);
    if (!fs.existsSync(versioned)) {
        execFileSync("cp", ["-a", "--reflink=auto", archivePath, versioned], { stdio: "inherit" });
    }
    return versioned;
}

export function assertTestCopyStopped(): void {
    const needle = Buffer.from(`${paths.testApp}/`);
    const pids: string[] = [];
    for (const entry of fs.readdirSync("/proc")) {
        if (!/^\d+$/.test(entry) || entry === String(process.pid)) continue;
        let command: Buffer;
        try {
            command = fs.readFileSync(`/proc/${entry}/cmdline`);
        } catch {
            continue;
        }
        if (command.includes(needle)) pids.push(entry);
    }
    if (pids.length > 0) {
        throw new Error(`quit the test copy before changing it (pids ${pids.join(", ")})`);
    }
}

function writeLink(copyDir: string): void {
    const link = {
        home: paths.root,
        runtime: paths.runtimeMain,
        patchVersion: PATCH_VERSION,
    };
    fs.writeFileSync(path.join(copyDir, LINK_NAME), `${JSON.stringify(link, null, 2)}\n`);
}

function prepareCopy(): { source: Detection; copy: Detection } {
    assertTestCopyStopped();
    const system = findSystemApp();
    const source = detect(system.appDir);
    ensureTestCopy(system.appDir);
    assertCopyIsIsolated(paths.testApp, system.appDir);
    const copy = detect(paths.testApp);
    if (
        copy.archive.main !== source.archive.main ||
        copy.archive.relativeArchive !== source.archive.relativeArchive
    ) {
        throw new Error("test copy startup entry does not match the system install");
    }
    return { source, copy };
}

function isUpToDate(state: InstallState | null, currentHash: string): state is InstallState {
    return (
        state?.status === "installed" &&
        state.target.path === paths.testApp &&
        state.files.archive.patchedSha256 === currentHash
    );
}

function verifyPatch(
    backupPath: string,
    archivePath: string,
    entry: string,
    stubLength: number,
): void {
    const backupArchive = openArchive(backupPath);
    const patchedArchive = openArchive(archivePath);
    try {
        const originalEntry = readEntry(backupArchive, entry);
        const patchedEntry = readEntry(patchedArchive, entry);
        if (!patchedEntry.subarray(stubLength).equals(originalEntry)) {
            throw new Error("patched startup entry does not keep the original program");
        }
        const originalManifest = readEntry(backupArchive, "package.json");
        const patchedManifest = readEntry(patchedArchive, "package.json");
        if (!patchedManifest.equals(originalManifest)) {
            throw new Error("patched archive changed package.json");
        }
    } finally {
        fs.closeSync(backupArchive.fd);
        fs.closeSync(patchedArchive.fd);
    }
}

interface PatchOutcome {
    changed: boolean;
    originalSha256: string;
    backupPath: string;
}

function patchStartupEntry(
    archivePath: string,
    entry: string,
    codexVersion: string,
    currentHash: string,
): PatchOutcome {
    if (entryIncludes(archivePath, entry, Buffer.from(SENTINEL))) {
        throw new Error(
            "startup entry already contains the loader but the archive is not the recorded patch; uninstall first",
        );
    }
    const backupPath = backupBaseline(archivePath, codexVersion, currentHash);
    const originalSha256 = sha256File(backupPath);
    if (originalSha256 !== currentHash) {
        throw new Error("backup does not match the archive that will be patched");
    }
    const stub = Buffer.from(loaderStub());
    const patched = patchEntry(archivePath, entry, (current) => {
        if (current.includes(Buffer.from(SENTINEL))) return current;
        return Buffer.concat([stub, current]);
    });
    if (!patched.bytes.includes(Buffer.from(SENTINEL))) {
        throw new Error("loader sentinel was not written into the startup entry");
    }
    if (patched.changed) verifyPatch(backupPath, archivePath, entry, stub.length);
    return { changed: patched.changed, originalSha256, backupPath };
}

function stateSource(source: Detection): InstallState["source"] {
    return {
        path: source.appDir,
        codexVersion: source.codexVersion,
        runtimeType: source.runtimeType,
        chromiumVersion: source.chromiumVersion,
        rpm: source.rpm,
    };
}

function stateApp(copy: Detection): InstallState["app"] {
    return {
        codexVersion: copy.codexVersion,
        runtimeType: copy.runtimeType,
        chromiumVersion: copy.chromiumVersion,
        archive: copy.archive.relativeArchive,
        entry: copy.archive.main,
    };
}

function stateArchive(
    copy: Detection,
    patched: PatchOutcome,
    archivePath: string,
): InstallState["files"]["archive"] {
    return {
        path: path.join(copy.archive.relativeArchive),
        originalSha256: patched.originalSha256,
        patchedSha256: sha256File(archivePath),
    };
}

function stateLoader(): InstallState["loader"] {
    return { sentinel: SENTINEL, link: LINK_NAME, runtime: paths.runtimeMain };
}

function buildState(
    source: Detection,
    copy: Detection,
    patched: PatchOutcome,
    archivePath: string,
): InstallState {
    return {
        schema: 1,
        status: "installed",
        patchVersion: PATCH_VERSION,
        installedAt: new Date().toISOString(),
        source: stateSource(source),
        target: { kind: "test-copy", path: paths.testApp },
        app: stateApp(copy),
        files: { archive: stateArchive(copy, patched, archivePath) },
        backup: { archive: patched.backupPath },
        loader: stateLoader(),
    };
}

export function install(): InstallResult {
    const { source, copy } = prepareCopy();
    const archivePath = copy.archive.archivePath;
    const currentHash = sha256File(archivePath);
    const existing = readState();
    if (isUpToDate(existing, currentHash)) {
        writeLink(paths.testApp);
        return { action: "unchanged", state: existing, source, copy };
    }
    const patched = patchStartupEntry(
        archivePath,
        copy.archive.main,
        copy.codexVersion,
        currentHash,
    );
    writeLink(paths.testApp);
    const state = buildState(source, copy, patched, archivePath);
    writeState(state);
    return { action: patched.changed ? "patched" : "already-patched", state, source, copy };
}
