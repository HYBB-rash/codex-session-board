import * as fs from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import { assertIsolatedPath, copyFile, linkFile, restoreMacMetadata } from "./copy.js";
import { locateApp } from "./detect.js";
import { assertTestCopyStopped, type InstallState, readState, writeState } from "./install.js";

export interface RestoreResult {
    state: InstallState;
    archiveHash: string;
}

export function restore(): RestoreResult {
    assertTestCopyStopped();
    const state = readState();
    if (!state) throw new Error("nothing is installed");
    const archivePath = path.join(state.target.path, state.files.archive.path);
    assertIsolatedPath(archivePath);
    const backupPath = state.backup.archive;
    if (!fs.existsSync(backupPath)) throw new Error(`backup is missing: ${backupPath}`);
    if (path.resolve(archivePath) === path.resolve(backupPath)) {
        throw new Error("backup and target are the same file");
    }
    if (sha256File(backupPath) !== state.files.archive.originalSha256) {
        throw new Error("stored archive backup no longer matches its checksum");
    }
    const currentHash = sha256File(archivePath);
    if (
        currentHash !== state.files.archive.patchedSha256 &&
        currentHash !== state.files.archive.originalSha256
    ) {
        throw new Error("the copy archive changed after installation");
    }
    const restored = `${archivePath}.restore`;
    assertIsolatedPath(restored);
    copyFile(backupPath, restored);
    fs.renameSync(restored, archivePath);
    const link = linkFile(locateApp(state.target.path).resourcesDir);
    assertIsolatedPath(link);
    fs.rmSync(link, { force: true });
    restoreMacMetadata(state.target.path, backupPath);
    const hash = sha256File(archivePath);
    if (hash !== state.files.archive.originalSha256) {
        throw new Error("restored archive hash does not match the recorded original");
    }
    state.status = "removed";
    state.removedAt = new Date().toISOString();
    writeState(state);
    return { state, archiveHash: hash };
}
