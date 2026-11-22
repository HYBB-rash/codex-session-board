#!/usr/bin/env node
"use strict";

const { detect, findSystemApp, formatDetection } = require("../installer/detect");
const { install } = require("../installer/install");
const { launch } = require("../installer/launch");
const { restore } = require("../installer/restore");
const { formatStatus, status } = require("../installer/status");

const command = process.argv[2];

function fail(error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}

if (command === "detect") {
  try {
    console.log(formatDetection(detect(findSystemApp().appDir)));
  } catch (error) {
    fail(error);
  }
} else if (command === "install") {
  try {
    const result = install();
    console.log(`action: ${result.action}`);
    console.log(`test copy: ${result.state.target.path}`);
    console.log(`codex version: ${result.state.app.codexVersion}`);
    console.log(`runtime: ${result.state.app.runtimeType}`);
    console.log(`entry: ${result.state.app.entry}`);
    console.log(`original sha256: ${result.state.files.archive.originalSha256}`);
    console.log(`patched sha256: ${result.state.files.archive.patchedSha256}`);
  } catch (error) {
    fail(error);
  }
} else if (command === "uninstall") {
  try {
    const result = restore();
    console.log("action: removed");
    console.log(`archive sha256: ${result.archiveHash}`);
  } catch (error) {
    fail(error);
  }
} else if (command === "status") {
  try {
    console.log(formatStatus(status()));
  } catch (error) {
    fail(error);
  }
} else if (command === "launch") {
  try {
    const pid = launch(process.argv.slice(3));
    console.log(`pid: ${pid}`);
  } catch (error) {
    fail(error);
  }
} else {
  console.log("Usage: codex-enhancer detect|install|uninstall|status|launch");
  process.exitCode = command ? 1 : 0;
}
