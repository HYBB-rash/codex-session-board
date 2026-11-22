"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { listAsarFiles, lookup, openArchive, readEntry } = require("./asar");

const FUSE_SENTINEL = "dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX";
const FUSE_NAMES = [
  "RunAsNode",
  "EnableCookieEncryption",
  "EnableNodeOptionsEnvironmentVariable",
  "EnableNodeCliInspectArguments",
  "EnableEmbeddedAsarIntegrityValidation",
  "OnlyLoadAppFromAsar",
  "LoadBrowserProcessSpecificV8Snapshot",
  "GrantFileProtocolExtraPrivileges",
  "WasmTrapHandlers",
];

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8").trim();
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function commandOutput(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

function resolveLauncher(launcherPath) {
  const resolved = fs.realpathSync(launcherPath);
  const text = fs.readFileSync(resolved, "utf8");
  const match = text.match(/exec\s+(\S*start\.sh)/);
  if (match) return path.resolve(path.dirname(resolved), match[1]);
  if (resolved.endsWith("start.sh")) return resolved;
  throw new Error(`could not resolve a start.sh from ${launcherPath}`);
}

function findSystemApp() {
  const candidates = [];
  if (fs.existsSync("/usr/bin/codex-desktop")) candidates.push("/usr/bin/codex-desktop");
  if (fs.existsSync("/opt/codex-desktop/start.sh")) candidates.push("/opt/codex-desktop/start.sh");
  if (candidates.length === 0) {
    throw new Error("Codex Desktop launcher was not found");
  }
  const startScript = resolveLauncher(candidates[0]);
  const appDir = path.dirname(startScript);
  const binary = path.join(appDir, "ChatGPT");
  if (!fs.existsSync(binary)) throw new Error(`ChatGPT binary is missing in ${appDir}`);
  return { appDir, startScript, binary };
}

function readFuses(binaryPath) {
  const fd = fs.openSync(binaryPath, "r");
  try {
    const size = fs.fstatSync(fd).size;
    const sentinel = Buffer.from(FUSE_SENTINEL);
    const chunkSize = 1024 * 1024;
    for (let offset = 0; offset < size; offset += chunkSize - sentinel.length) {
      const length = Math.min(chunkSize, size - offset);
      const chunk = Buffer.alloc(length);
      fs.readSync(fd, chunk, 0, length, offset);
      const index = chunk.indexOf(sentinel);
      if (index < 0) continue;
      const slice = Buffer.alloc(64);
      fs.readSync(fd, slice, 0, slice.length, offset + index);
      const version = slice[sentinel.length];
      const count = slice[sentinel.length + 1];
      const states = slice.subarray(sentinel.length + 2, sentinel.length + 2 + count).toString("ascii");
      return {
        version,
        states,
        flags: Object.fromEntries(
          [...states].map((state, fuseIndex) => [FUSE_NAMES[fuseIndex] ?? `fuse${fuseIndex}`, state === "1"]),
        ),
      };
    }
  } finally {
    fs.closeSync(fd);
  }
  return null;
}

function findStartupArchive(appDir) {
  const resourcesDir = path.join(appDir, "resources");
  const names = listAsarFiles(resourcesDir);
  const matches = [];
  for (const name of names) {
    const archivePath = path.join(resourcesDir, name);
    const archive = openArchive(archivePath);
    try {
      if (!lookup(archive.header, "package.json")) continue;
      const manifest = JSON.parse(readEntry(archive, "package.json").toString("utf8"));
      if (typeof manifest.main !== "string" || !lookup(archive.header, manifest.main)) continue;
      matches.push({
        archivePath,
        relativeArchive: path.posix.join("resources", name),
        main: manifest.main,
        packageName: manifest.name ?? null,
        packageVersion: manifest.version ?? null,
      });
    } finally {
      fs.closeSync(archive.fd);
    }
  }
  if (matches.length !== 1) {
    throw new Error(`expected one app archive with a main entry, found ${matches.length}`);
  }
  return matches[0];
}

function detect(appDir = findSystemApp().appDir) {
  const located = appDir === findSystemApp().appDir ? findSystemApp() : {
    appDir,
    startScript: path.join(appDir, "start.sh"),
    binary: path.join(appDir, "ChatGPT"),
  };
  const metadataPath = path.join(located.appDir, "resources", "linux-package-metadata.json");
  const owlPath = path.join(located.appDir, "resources", "owl-electron-app.json");
  const versionPath = path.join(located.appDir, "version");
  if (!fs.existsSync(metadataPath) || !fs.existsSync(owlPath) || !fs.existsSync(versionPath)) {
    throw new Error(`Codex metadata is incomplete in ${located.appDir}`);
  }
  const metadata = readJson(metadataPath);
  const owl = readJson(owlPath);
  const startup = findStartupArchive(located.appDir);
  return {
    appDir: located.appDir,
    startScript: located.startScript,
    binary: located.binary,
    codexVersion: metadata.version,
    buildFlavor: metadata.codexBuildFlavor ?? null,
    chromiumVersion: readText(versionPath),
    runtimeType: owl.runtimeName ?? null,
    runtimeArchiveSha: owl.runtimeArchiveSha ?? null,
    rpm: commandOutput("rpm", ["-q", "codex-desktop"]),
    fuses: readFuses(located.binary),
    archive: startup,
  };
}

function formatDetection(info) {
  const fuseLine = info.fuses
    ? Object.entries(info.fuses.flags)
        .map(([name, enabled]) => `${name}=${enabled ? "on" : "off"}`)
        .join(", ")
    : "not found";
  return [
    `path: ${info.appDir}`,
    `codex version: ${info.codexVersion}`,
    `runtime: ${info.runtimeType}`,
    `chromium: ${info.chromiumVersion}`,
    `package: ${info.rpm ?? "not an rpm"}`,
    `archive: ${info.archive.relativeArchive}`,
    `entry: ${info.archive.main}`,
    `fuses: ${fuseLine}`,
  ].join("\n");
}

module.exports = {
  detect,
  findSystemApp,
  formatDetection,
};
