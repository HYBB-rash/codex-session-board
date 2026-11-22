"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { openArchive, readEntry, sha256File } = require("./asar");
const { SENTINEL } = require("./constants");
const { detect, findSystemApp } = require("./detect");
const { readState } = require("./install");

function entryHasSentinel(archivePath, entryPath) {
  const archive = openArchive(archivePath);
  try {
    return readEntry(archive, entryPath).includes(Buffer.from(SENTINEL));
  } finally {
    fs.closeSync(archive.fd);
  }
}

function assess(state, current) {
  const reasons = [];
  if (!state) return { result: "not-installed", reasons };
  const archivePath = path.join(state.target.path, state.files.archive.path);
  if (!fs.existsSync(archivePath)) {
    return { result: "patch-stale", reasons: ["archive-missing"] };
  }
  const hash = sha256File(archivePath);
  const sentinel = entryHasSentinel(archivePath, state.app.entry);
  if (state.status === "removed") {
    if (hash !== state.files.archive.originalSha256) reasons.push("archive-hash-differs-from-original");
    if (sentinel) reasons.push("loader-still-present");
    return { result: reasons.length === 0 ? "removed" : "patch-stale", reasons, hash, sentinel };
  }
  if (hash !== state.files.archive.patchedSha256) reasons.push("archive-hash-changed");
  if (!sentinel) reasons.push("loader-missing");
  if (current.codexVersion !== state.app.codexVersion) reasons.push("copy-version-changed");
  if (current.runtimeType !== state.app.runtimeType) reasons.push("runtime-type-changed");
  if (current.archive.main !== state.app.entry) reasons.push("startup-entry-changed");
  return {
    result: reasons.length === 0 ? "installed" : "patch-stale",
    reasons,
    hash,
    sentinel,
  };
}

function status() {
  const state = readState();
  const system = detect(findSystemApp().appDir);
  let copy = null;
  let assessment = { result: "not-installed", reasons: [] };
  if (state && fs.existsSync(state.target.path)) {
    copy = detect(state.target.path);
    assessment = assess(state, copy);
  } else if (state) {
    assessment = { result: "patch-stale", reasons: ["test-copy-missing"] };
  }
  const upstreamChanged = Boolean(state && system.codexVersion !== state.source.codexVersion);
  return { state, system, copy, assessment, upstreamChanged };
}

function formatStatus(report) {
  const lines = [
    `result: ${report.assessment.result}`,
    `system version: ${report.system.codexVersion}`,
    `system runtime: ${report.system.runtimeType}`,
  ];
  if (report.upstreamChanged) {
    lines.push(`upstream changed: ${report.state.source.codexVersion} -> ${report.system.codexVersion}`);
  }
  if (report.assessment.reasons?.length) lines.push(`reasons: ${report.assessment.reasons.join(", ")}`);
  if (report.copy) lines.push(`test copy: ${report.copy.appDir}`);
  return lines.join("\n");
}

module.exports = {
  assess,
  formatStatus,
  status,
};
