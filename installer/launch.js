"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { APP_ID } = require("./constants");
const { readState } = require("./install");
const paths = require("./paths");

function launch(extraArgs = []) {
  const state = readState();
  if (!state || state.status !== "installed") {
    throw new Error("install the test copy before launching it");
  }
  const startScript = path.join(state.target.path, "start.sh");
  if (!fs.existsSync(startScript)) throw new Error(`test launcher is missing: ${startScript}`);
  fs.mkdirSync(paths.userData, { recursive: true });
  fs.mkdirSync(paths.codexHome, { recursive: true });
  fs.mkdirSync(paths.logs, { recursive: true });
  const child = spawn(startScript, ["--user-data-dir", paths.userData, ...extraArgs], {
    cwd: state.target.path,
    detached: true,
    stdio: ["ignore", "inherit", "inherit"],
    env: {
      ...process.env,
      CODEX_LINUX_APP_ID: APP_ID,
      CODEX_LINUX_APP_DISPLAY_NAME: "Codex Enhancer POC",
      CODEX_HOME: paths.codexHome,
      CODEX_LINUX_DISABLE_USAGE_REPORTING: "1",
      CODEX_ENHANCER_AUTO_OPEN: process.env.CODEX_ENHANCER_AUTO_OPEN ?? "0",
    },
  });
  child.unref();
  return child.pid;
}

module.exports = { launch };
