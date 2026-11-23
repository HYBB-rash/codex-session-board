import * as fs from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import {
    assertIsolatedPath,
    assertMacMetadataAvailable,
    copyFile,
    linkFile,
    restoreMacMetadata,
} from "./copy.js";
import {
    assertTestCopyStopped,
    type InstallState,
    readState,
    restoreEmbeddedMac,
    withRecoveredMacCopy,
    writeState,
} from "./install.js";

export interface RestoreResult {
    state: InstallState;
    archiveHash: string;
}

function restoreCopy(state: InstallState | null): RestoreResult {
    assertTestCopyStopped();
    if (!state) throw new Error("nothing is installed");
    if (state.mac) {
        return {
            state: restoreEmbeddedMac(state),
            archiveHash: state.files.archive.originalSha256,
        };
    }
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
    const link = linkFile(path.dirname(archivePath));
    assertIsolatedPath(link);
    assertMacMetadataAvailable(backupPath);
    const restored = `${archivePath}.restore`;
    assertIsolatedPath(restored);
    copyFile(backupPath, restored);
    fs.renameSync(restored, archivePath);
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

export function restore(): RestoreResult {
    if (process.platform === "darwin") return withRecoveredMacCopy(restoreCopy);
    return restoreCopy(readState());
}
