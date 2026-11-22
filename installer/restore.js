"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { sha256File } = require("./asar");
const { LINK_NAME } = require("./constants");
const { assertTestCopyStopped, readState, writeState } = require("./install");
const paths = require("./paths");

function restore() {
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

module.exports = { restore };
