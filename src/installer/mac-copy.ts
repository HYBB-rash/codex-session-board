import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import { assertIsolatedPath, assertSeparateFromSource } from "./copy.js";
import { findSystemApp } from "./detect.js";
import type { InstallState } from "./install.js";
import { verifyMacSignature, verifyOfficialCopy } from "./mac-signing.js";
import * as paths from "./paths.js";
import { withDirectoryLock } from "./releases.js";

export const macWorkspace = path.join(paths.stateDir, ".mac-copy");
export const macCandidate = path.join(macWorkspace, "candidate.app");
export const macPending = path.join(macWorkspace, "pending.json");
const displaced = path.join(macWorkspace, "previous.app");

export function withMacCopyLock<Result>(operation: () => Result): Result {
    assertSeparateFromSource(paths.stateDir, findSystemApp().appDir);
    fs.mkdirSync(paths.stateDir, { recursive: true, mode: 0o700 });
    return withDirectoryLock(path.join(paths.stateDir, ".mac-copy-lock"), operation);
}

export function hasBundle(entry: string): boolean {
    const stat = fs.lstatSync(entry, { throwIfNoEntry: false });
    if (stat && !stat.isDirectory()) throw new Error(`expected a macOS app directory at ${entry}`);
    return Boolean(stat);
}

export function dittoBundle(source: string, destination: string): void {
    if (hasBundle(destination)) throw new Error(`macOS bundle already exists at ${destination}`);
    execFileSync("/usr/bin/ditto", [source, destination], { stdio: "inherit" });
}

export function officialBackup(codeHash: string): string {
    if (!/^[a-f0-9]{40}$/.test(codeHash)) throw new Error("invalid official macOS signature hash");
    return path.join(paths.backupDir, `mac-${codeHash}.app`);
}

export function verifyBundleState(bundle: string, state: InstallState): void {
    const receipt = state.mac;
    if (!receipt) throw new Error("missing macOS bundle receipt");
    const archive = path.join(bundle, state.files.archive.path);
    const removed = state.status === "removed";
    const expectedArchive = removed
        ? state.files.archive.originalSha256
        : state.files.archive.patchedSha256;
    if (!hasBundle(bundle) || sha256File(archive) !== expectedArchive) {
        throw new Error(`macOS archive does not match the installation receipt at ${bundle}`);
    }
    const signature = removed ? verifyOfficialCopy(bundle, archive) : verifyMacSignature(bundle);
    const expectedCodeHash = removed ? receipt.originalCodeHash : receipt.patchedCodeHash;
    if (signature !== expectedCodeHash) throw new Error("macOS signature differs from the receipt");
}

export function recoverMacCopy(
    readPending: () => InstallState | null,
    persist: (state: InstallState) => void,
): void {
    assertIsolatedPath(paths.testApp);
    if (!hasBundle(macWorkspace)) return;
    const pending = readPending();
    const candidate = hasBundle(macCandidate);
    const target = hasBundle(paths.testApp);
    const previous = hasBundle(displaced);
    if (pending && !candidate) {
        if (!target) throw new Error("missing macOS bundle after publication");
        verifyBundleState(paths.testApp, pending);
        persist(pending);
    } else if (previous) {
        if (!pending || target) throw new Error("conflicting macOS copy transaction paths");
        fs.renameSync(displaced, paths.testApp);
    }
    fs.rmSync(displaced, { recursive: true, force: true });
    fs.rmSync(macPending, { force: true });
    fs.rmSync(macWorkspace, { recursive: true });
}

export function publishMacCopy(state: InstallState, hadPrevious: boolean): void {
    assertIsolatedPath(paths.testApp);
    if (hasBundle(paths.testApp) !== hadPrevious) {
        throw new Error("macOS copy target changed while preparing the transaction");
    }
    const pending = `${macPending}.next`;
    fs.writeFileSync(pending, `${JSON.stringify(state, null, 2)}\n`, {
        mode: 0o600,
        flag: "wx",
        flush: true,
    });
    fs.renameSync(pending, macPending);
    if (hadPrevious) fs.renameSync(paths.testApp, displaced);
    fs.renameSync(macCandidate, paths.testApp);
}
