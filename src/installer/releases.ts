import { randomUUID } from "node:crypto";
import {
    chmodSync,
    chownSync,
    cpSync,
    existsSync,
    mkdirSync,
    mkdtempSync,
    readdirSync,
    readFileSync,
    renameSync,
    rmdirSync,
    rmSync,
    unlinkSync,
    writeFileSync,
} from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import { inPlaceReleaseStore } from "./in-place-receipt.js";
import { assertSystemPath } from "./in-place-target.js";
import * as paths from "./paths.js";
import {
    type ReleaseManifest,
    readReleaseManifest,
    releaseManifestName,
    releasePlatform,
    verifyInPlaceRelease,
    verifyRelease,
} from "./release-manifest.js";

interface ReleasePointer {
    current: string;
    previous: string | null;
}

export interface UpdateStatus {
    available: boolean;
    phase?: "up_to_date" | "active" | "awaiting_desktop_reload" | "failed";
    version?: string;
    runningVersion?: string;
    error?: string;
    failedStage?: "verifying" | "installing";
}

export function releaseStore(): string {
    return paths.inPlaceRuntime ? inPlaceReleaseStore() : path.join(paths.stateDir, "releases");
}

export function releasePointer(store: string): ReleasePointer | null {
    const file = path.join(store, "active.json");
    if (!existsSync(file)) return null;
    const value: unknown = JSON.parse(readFileSync(file, "utf8"));
    if (typeof value !== "object" || value === null) throw new Error("invalid release activation");
    const entry = value as Partial<ReleasePointer>;
    const valid = (id: unknown) => typeof id === "string" && /^[a-f0-9]{64}$/.test(id);
    if (!valid(entry.current) || (entry.previous !== null && !valid(entry.previous))) {
        throw new Error("invalid release activation");
    }
    return entry as ReleasePointer;
}

function writePointer(store: string, pointer: ReleasePointer): void {
    const temporary = path.join(store, `active-${randomUUID()}.tmp`);
    try {
        writeFileSync(temporary, `${JSON.stringify(pointer)}\n`, {
            mode: store === inPlaceReleaseStore() ? 0o644 : 0o600,
            flush: true,
        });
        renameSync(temporary, path.join(store, "active.json"));
    } finally {
        rmSync(temporary, { force: true });
    }
}

function verifyStoreRelease(store: string, packageRoot: string): ReleaseManifest {
    return store === inPlaceReleaseStore()
        ? verifyInPlaceRelease(packageRoot)
        : verifyRelease(packageRoot);
}

function loadRelease(store: string, id: string): ReleaseManifest {
    const release = path.join(store, id);
    if (sha256File(path.join(release, releaseManifestName)) !== id) {
        throw new Error("active release manifest does not match its identity");
    }
    return verifyStoreRelease(store, release);
}

function stageRelease(packageRoot: string, store: string, id: string): void {
    if (existsSync(path.join(store, id))) {
        if (store === inPlaceReleaseStore())
            assertSystemPath(path.join(store, id, releaseManifestName), false);
        loadRelease(store, id);
        return;
    }
    const temporary = path.join(store, `stage-${randomUUID()}`);
    try {
        mkdirSync(temporary, { mode: store === inPlaceReleaseStore() ? 0o755 : 0o700 });
        cpSync(path.join(packageRoot, "dist"), path.join(temporary, "dist"), { recursive: true });
        cpSync(
            path.join(packageRoot, releaseManifestName),
            path.join(temporary, releaseManifestName),
        );
        if (sha256File(path.join(temporary, releaseManifestName)) !== id) {
            throw new Error("release manifest changed while staging");
        }
        verifyStoreRelease(store, temporary);
        if (store === inPlaceReleaseStore()) {
            for (const member of readdirSync(temporary, { recursive: true, withFileTypes: true })) {
                const installed = path.join(member.parentPath, member.name);
                chownSync(installed, 0, 0);
                chmodSync(
                    installed,
                    member.isDirectory() || member.name === "codex-session-board.js"
                        ? 0o755
                        : 0o644,
                );
            }
        }
        renameSync(temporary, path.join(store, id));
    } finally {
        rmSync(temporary, { recursive: true, force: true });
    }
}

function removeLock(lock: string, owner: string): void {
    try {
        unlinkSync(path.join(lock, owner));
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
        throw error;
    }
    try {
        rmdirSync(lock);
    } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code !== "ENOENT" && code !== "ENOTEMPTY") throw error;
    }
}

function clearExitedOwner(lock: string, operation: string): void {
    let names: string[];
    try {
        names = readdirSync(lock);
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
        throw error;
    }
    const owner = names[0];
    if (!owner) return;
    const pid = /^(\d+)-[a-f0-9-]+$/.exec(owner)?.[1];
    if (!pid || names.length !== 1) throw new Error(`unknown transaction lock at ${lock}`);
    try {
        process.kill(Number(pid), 0);
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
        removeLock(lock, owner);
        return;
    }
    throw new Error(`another ${operation} is in progress (pid ${pid})`);
}

function acquireLock(store: string, lock: string, owner: string, operation: string): void {
    const candidate = mkdtempSync(path.join(store, ".lock-"));
    try {
        writeFileSync(path.join(candidate, owner), "");
        try {
            renameSync(candidate, lock);
        } catch (error) {
            const code = (error as NodeJS.ErrnoException).code;
            if (code !== "ENOTEMPTY" && code !== "EEXIST") throw error;
            clearExitedOwner(lock, operation);
            renameSync(candidate, lock);
        }
    } finally {
        rmSync(candidate, { recursive: true, force: true });
    }
}

export function withDirectoryLock<Result>(
    lock: string,
    change: () => Result,
    operation = "transaction",
): Result {
    const store = path.dirname(lock);
    const owner = `${process.pid}-${randomUUID()}`;
    acquireLock(store, lock, owner, operation);
    try {
        return change();
    } finally {
        removeLock(lock, owner);
    }
}

function changeRelease<Result>(store: string, change: () => Result): Result {
    mkdirSync(store, { recursive: true, mode: store === inPlaceReleaseStore() ? 0o755 : 0o700 });
    return withDirectoryLock(path.join(store, ".activating"), change, "update or rollback");
}

export function activateRelease(packageRoot: string, store = releaseStore()): ReleaseManifest {
    return changeRelease(store, () => {
        const manifest = verifyStoreRelease(store, packageRoot);
        const id = sha256File(path.join(packageRoot, releaseManifestName));
        stageRelease(packageRoot, store, id);
        const previous = releasePointer(store)?.current ?? null;
        if (previous !== id) writePointer(store, { current: id, previous });
        return manifest;
    });
}

export function rollbackRelease(store = releaseStore()): ReleaseManifest {
    return changeRelease(store, () => {
        const pointer = releasePointer(store);
        if (!pointer?.previous) throw new Error("no earlier board release is installed");
        const manifest = loadRelease(store, pointer.previous);
        writePointer(store, { current: pointer.previous, previous: pointer.current });
        return manifest;
    });
}

export function activeRelease(store = releaseStore()): ReleaseManifest | null {
    const pointer = releasePointer(store);
    return pointer && loadRelease(store, pointer.current);
}

export function runningVersion(): string {
    const manifest = runningManifest();
    if (!existsSync(manifest)) return "dev";
    return readReleaseManifest(manifest).version;
}

function runningManifest(): string {
    return path.join(path.dirname(paths.outputDir), releaseManifestName);
}

function updateStatus(start: boolean): UpdateStatus {
    const running = runningVersion();
    if (paths.inPlaceRuntime) {
        const installed = activeRelease();
        const current = runningManifest();
        if (!installed || sha256File(current) === releasePointer(releaseStore())?.current)
            return { available: false, runningVersion: running };
        return {
            available: true,
            phase: "awaiting_desktop_reload",
            runningVersion: running,
            version: installed.version,
        };
    }
    const packageRoot = path.join(paths.stateDir, "update-candidate");
    const manifest = path.join(packageRoot, releaseManifestName);
    if (!existsSync(manifest)) return { available: false, runningVersion: running };
    const candidate = readReleaseManifest(manifest);
    if (candidate.platform !== releasePlatform())
        throw new Error(`update candidate is for ${candidate.platform}`);
    if (start) activateRelease(packageRoot);
    const active = releasePointer(releaseStore())?.current;
    const target = sha256File(manifest);
    const result = { available: true, version: candidate.version, runningVersion: running };
    if (active !== target) return result;
    const current = runningManifest();
    const loaded = existsSync(current) && sha256File(current) === active;
    return { ...result, phase: loaded ? "active" : "awaiting_desktop_reload" };
}

export function boardUpdateStatus(start = false): UpdateStatus {
    try {
        return updateStatus(start);
    } catch (error) {
        return {
            available: true,
            phase: "failed",
            failedStage: start ? "installing" : "verifying",
            error: error instanceof Error ? error.message : String(error),
        };
    }
}
