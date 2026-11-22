import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import { LINK_NAME } from "./constants.js";
import { assertTestCopyStopped, readState, writeState, type InstallState } from "./install.js";
import * as paths from "./paths.js";

export interface RestoreResult {
  state: InstallState;
  archiveHash: string;
}

export function restore(): RestoreResult {
  assertTestCopyStopped();
  const state = readState();
  if (!state) throw new Error("nothing is installed");
  const archivePath = path.join(state.target.path, state.files.archive.path);
  const backupPath = state.backup.archive;
  if (!fs.existsSync(backupPath)) throw new Error(`backup is missing: ${backupPath}`);
  if (path.resolve(archivePath) === path.resolve(backupPath)) {
    throw new Error("backup and target are the same file");
  }
  const restored = `${archivePath}.restore`;
  execFileSync("cp", ["-a", "--reflink=auto", backupPath, restored], { stdio: "inherit" });
  fs.renameSync(restored, archivePath);
  fs.rmSync(path.join(state.target.path, LINK_NAME), { force: true });
  const hash = sha256File(archivePath);
  if (hash !== state.files.archive.originalSha256) {
    throw new Error("restored archive hash does not match the recorded original");
  }
  state.status = "removed";
  state.removedAt = new Date().toISOString();
  writeState(state);
  return { state, archiveHash: hash };
}
