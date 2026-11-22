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

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function recordField(
    value: Record<string, unknown>,
    key: string,
    label: string,
): Record<string, unknown> {
    const child = value[key];
    if (!isRecord(child)) throw new Error(`install state ${label} is not an object`);
    return child;
}

function stringField(value: Record<string, unknown>, key: string, label: string): string {
    const child = value[key];
    if (typeof child !== "string") throw new Error(`install state ${label} is not a string`);
    return child;
}

function nullableStringField(
    value: Record<string, unknown>,
    key: string,
    label: string,
): string | null {
    const child = value[key];
    if (child === null) return null;
    if (typeof child !== "string") throw new Error(`install state ${label} is not a string`);
    return child;
}

function parseState(value: unknown): InstallState {
    if (!isRecord(value)) throw new Error("install state is not an object");
    if (value["schema"] !== 1) throw new Error("install state schema is not 1");
    const status = value["status"];
    if (status !== "installed" && status !== "removed") {
        throw new Error("install state status is not installed or removed");
    }
    const removedAt = value["removedAt"];
    if (removedAt !== undefined && typeof removedAt !== "string") {
        throw new Error("install state removedAt is not a string");
    }
    const source = recordField(value, "source", "source");
    const target = recordField(value, "target", "target");
    const app = recordField(value, "app", "app");
    const files = recordField(value, "files", "files");
    const archive = recordField(files, "archive", "files.archive");
    const backup = recordField(value, "backup", "backup");
    const loader = recordField(value, "loader", "loader");
    if (target["kind"] !== "test-copy")
        throw new Error("install state target.kind is not test-copy");
    const state: InstallState = {
        schema: 1,
        status,
        patchVersion: stringField(value, "patchVersion", "patchVersion"),
        installedAt: stringField(value, "installedAt", "installedAt"),
        source: {
            path: stringField(source, "path", "source.path"),
            codexVersion: stringField(source, "codexVersion", "source.codexVersion"),
            runtimeType: nullableStringField(source, "runtimeType", "source.runtimeType"),
            chromiumVersion: stringField(source, "chromiumVersion", "source.chromiumVersion"),
            rpm: nullableStringField(source, "rpm", "source.rpm"),
        },
        target: {
            kind: "test-copy",
            path: stringField(target, "path", "target.path"),
        },
        app: {
            codexVersion: stringField(app, "codexVersion", "app.codexVersion"),
            runtimeType: nullableStringField(app, "runtimeType", "app.runtimeType"),
            chromiumVersion: stringField(app, "chromiumVersion", "app.chromiumVersion"),
            archive: stringField(app, "archive", "app.archive"),
            entry: stringField(app, "entry", "app.entry"),
        },
        files: {
            archive: {
                path: stringField(archive, "path", "files.archive.path"),
                originalSha256: stringField(
                    archive,
                    "originalSha256",
                    "files.archive.originalSha256",
                ),
                patchedSha256: stringField(archive, "patchedSha256", "files.archive.patchedSha256"),
            },
        },
        backup: {
            archive: stringField(backup, "archive", "backup.archive"),
        },
        loader: {
            sentinel: stringField(loader, "sentinel", "loader.sentinel"),
            link: stringField(loader, "link", "loader.link"),
            runtime: stringField(loader, "runtime", "loader.runtime"),
        },
    };
    if (typeof removedAt === "string") state.removedAt = removedAt;
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

export function install(): InstallResult {
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

    const archivePath = copy.archive.archivePath;
    const existing = readState();
    const currentHash = sha256File(archivePath);
    if (
        existing?.status === "installed" &&
        existing.target.path === paths.testApp &&
        existing.files.archive.patchedSha256 === currentHash
    ) {
        writeLink(paths.testApp);
        return { action: "unchanged", state: existing, source, copy };
    }
    if (entryIncludes(archivePath, copy.archive.main, Buffer.from(SENTINEL))) {
        throw new Error(
            "startup entry already contains the loader but the archive is not the recorded patch; uninstall first",
        );
    }

    const backupPath = backupBaseline(archivePath, copy.codexVersion, currentHash);
    const originalSha256 = sha256File(backupPath);
    if (originalSha256 !== currentHash) {
        throw new Error("backup does not match the archive that will be patched");
    }

    const stub = Buffer.from(loaderStub());
    const patched = patchEntry(archivePath, copy.archive.main, (current) => {
        if (current.includes(Buffer.from(SENTINEL))) return current;
        return Buffer.concat([stub, current]);
    });
    if (!patched.bytes.includes(Buffer.from(SENTINEL))) {
        throw new Error("loader sentinel was not written into the startup entry");
    }
    if (patched.changed) {
        const backupArchive = openArchive(backupPath);
        const patchedArchive = openArchive(archivePath);
        try {
            const originalEntry = readEntry(backupArchive, copy.archive.main);
            const patchedEntry = readEntry(patchedArchive, copy.archive.main);
            if (!patchedEntry.subarray(stub.length).equals(originalEntry)) {
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
    writeLink(paths.testApp);

    const state: InstallState = {
        schema: 1,
        status: "installed",
        patchVersion: PATCH_VERSION,
        installedAt: new Date().toISOString(),
        source: {
            path: source.appDir,
            codexVersion: source.codexVersion,
            runtimeType: source.runtimeType,
            chromiumVersion: source.chromiumVersion,
            rpm: source.rpm,
        },
        target: {
            kind: "test-copy",
            path: paths.testApp,
        },
        app: {
            codexVersion: copy.codexVersion,
            runtimeType: copy.runtimeType,
            chromiumVersion: copy.chromiumVersion,
            archive: copy.archive.relativeArchive,
            entry: copy.archive.main,
        },
        files: {
            archive: {
                path: path.join(copy.archive.relativeArchive),
                originalSha256,
                patchedSha256: sha256File(archivePath),
            },
        },
        backup: {
            archive: backupPath,
        },
        loader: {
            sentinel: SENTINEL,
            link: LINK_NAME,
            runtime: paths.runtimeMain,
        },
    };
    writeState(state);
    return { action: patched.changed ? "patched" : "already-patched", state, source, copy };
}
