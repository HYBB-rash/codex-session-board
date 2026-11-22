"use strict";

const path = require("node:path");

const root = path.resolve(__dirname, "..");

module.exports = {
  root,
  runtimeMain: path.join(root, "runtime", "main.js"),
  stateDir: path.join(root, "state"),
  testApp: path.join(root, "state", "test-app"),
  backupDir: path.join(root, "state", "backups"),
  installState: path.join(root, "state", "install.json"),
  logs: path.join(root, "logs"),
  userData: path.join(root, "state", "user-data"),
  codexHome: path.join(root, "state", "codex-home"),
};
