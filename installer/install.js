"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { openArchive, patchEntry, readEntry, sha256File } = require("./asar");
const { LINK_NAME, PATCH_VERSION, SENTINEL, loaderStub } = require("./constants");
const { detect, findSystemApp } = require("./detect");
const paths = require("./paths");

function readState() {
  if (!fs.existsSync(paths.installState)) return null;
  return JSON.parse(fs.readFileSync(paths.installState, "utf8"));
}

function writeState(state) {
  fs.mkdirSync(paths.stateDir, { recursive: true });
  const next = `${paths.installState}.next`;
  fs.writeFileSync(next, `${JSON.stringify(state, null, 2)}\n`);
  fs.renameSync(next, paths.installState);
}

function samePath(left, right) {
  return path.resolve(left) === path.resolve(right);
}

function assertCopyIsIsolated(copyDir, systemDir) {
  const copy = path.resolve(copyDir);
  const system = path.resolve(systemDir);
  if (copy === system || copy.startsWith(`${system}${path.sep}`)) {
    throw new Error(`refusing to modify the installed Codex at ${system}`);
  }
}

function ensureTestCopy(systemDir) {
  assertCopyIsIsolated(paths.testApp, systemDir);
  if (fs.existsSync(path.join(paths.testApp, "ChatGPT")) && fs.existsSync(path.join(paths.testApp, "start.sh"))) {
    return false;
  }
  fs.rmSync(paths.testApp, { recursive: true, force: true });
  fs.mkdirSync(paths.stateDir, { recursive: true });
  execFileSync("cp", ["-a", "--reflink=auto", systemDir, paths.testApp], { stdio: "inherit" });
  return true;
}

function entryContainsSentinel(archivePath, entryPath) {
  const archive = openArchive(archivePath);
  try {
    return readEntry(archive, entryPath).includes(Buffer.from(SENTINEL));
  } finally {
    fs.closeSync(archive.fd);
  }
}

function backupBaseline(archivePath, version, currentHash) {
  const directory = path.join(paths.backupDir, version);
  fs.mkdirSync(directory, { recursive: true });
  const stable = path.join(directory, "app.asar");
  if (!fs.existsSync(stable)) {
    execFileSync("cp", ["-a", "--reflink=auto", archivePath, stable], { stdio: "inherit" });
    return stable;
  }
  if (sha256File(stable) === currentHash) return stable;
  const versioned = path.join(directory, `${currentHash}.asar`);
  if (!fs.existsSync(versioned)) {
    execFileSync("cp", ["-a", "--reflink=auto", archivePath, versioned], { stdio: "inherit" });
  }
  return versioned;
}

function assertTestCopyStopped() {
  const needle = Buffer.from(`${paths.testApp}/`);
  const pids = [];
  for (const entry of fs.readdirSync("/proc")) {
    if (!/^\d+$/.test(entry) || entry === String(process.pid)) continue;
    let command;
    try {
      command = fs.readFileSync(`/proc/${entry}/cmdline`);
    } catch {
      continue;
    }
    if (command.includes(needle)) pids.push(entry);
  }
  if (pids.length > 0) {
    throw new Error(`quit the test copy before changing it (pids ${pids.join(", ")})`);
  }
}

function writeLink(copyDir) {
  const link = {
    home: paths.root,
    runtime: paths.runtimeMain,
    patchVersion: PATCH_VERSION,
  };
  fs.writeFileSync(path.join(copyDir, LINK_NAME), `${JSON.stringify(link, null, 2)}\n`);
}

function install() {
  assertTestCopyStopped();
  const system = findSystemApp();
  const source = detect(system.appDir);
  ensureTestCopy(system.appDir);
  assertCopyIsIsolated(paths.testApp, system.appDir);
  const copy = detect(paths.testApp);
  if (copy.archive.main !== source.archive.main || copy.archive.relativeArchive !== source.archive.relativeArchive) {
    throw new Error("test copy startup entry does not match the system install");
  }

  const archivePath = copy.archive.archivePath;
  const existing = readState();
  const currentHash = sha256File(archivePath);
  if (
    existing?.status === "installed" &&
    existing.target?.path === paths.testApp &&
    existing.files?.archive?.patchedSha256 === currentHash
  ) {
    writeLink(paths.testApp);
    return { action: "unchanged", state: existing, source, copy };
  }
  if (entryContainsSentinel(archivePath, copy.archive.main)) {
    throw new Error("startup entry already contains the loader but the archive is not the recorded patch; uninstall first");
  }

  const backupPath = backupBaseline(archivePath, copy.codexVersion, currentHash);
  const originalSha256 = sha256File(backupPath);
  if (originalSha256 !== currentHash) {
    throw new Error("backup does not match the archive that will be patched");
  }

  const stub = Buffer.from(loaderStub());
  const patched = patchEntry(archivePath, copy.archive.main, (current) => {
    if (current.includes(Buffer.from(SENTINEL))) return current;
    return Buffer.concat([stub, current]);
  });
  if (!patched.bytes.includes(Buffer.from(SENTINEL))) {
    throw new Error("loader sentinel was not written into the startup entry");
  }
  if (patched.changed) {
    const backupArchive = openArchive(backupPath);
    const patchedArchive = openArchive(archivePath);
    try {
      const originalEntry = readEntry(backupArchive, copy.archive.main);
      const patchedEntry = readEntry(patchedArchive, copy.archive.main);
      if (!patchedEntry.subarray(stub.length).equals(originalEntry)) {
        throw new Error("patched startup entry does not keep the original program");
      }
      const originalManifest = readEntry(backupArchive, "package.json");
      const patchedManifest = readEntry(patchedArchive, "package.json");
      if (!patchedManifest.equals(originalManifest)) {
        throw new Error("patched archive changed package.json");
      }
    } finally {
      fs.closeSync(backupArchive.fd);
      fs.closeSync(patchedArchive.fd);
    }
  }
  writeLink(paths.testApp);

  const state = {
    schema: 1,
    status: "installed",
    patchVersion: PATCH_VERSION,
    installedAt: new Date().toISOString(),
    source: {
      path: source.appDir,
      codexVersion: source.codexVersion,
      runtimeType: source.runtimeType,
      chromiumVersion: source.chromiumVersion,
      rpm: source.rpm,
    },
    target: {
      kind: "test-copy",
      path: paths.testApp,
    },
    app: {
      codexVersion: copy.codexVersion,
      runtimeType: copy.runtimeType,
      chromiumVersion: copy.chromiumVersion,
      archive: copy.archive.relativeArchive,
      entry: copy.archive.main,
    },
    files: {
      archive: {
        path: path.join(copy.archive.relativeArchive),
        originalSha256,
        patchedSha256: sha256File(archivePath),
      },
    },
    backup: {
      archive: backupPath,
    },
    loader: {
      sentinel: SENTINEL,
      link: LINK_NAME,
      runtime: paths.runtimeMain,
    },
  };
  writeState(state);
  return { action: patched.changed ? "patched" : "already-patched", state, source, copy };
}

module.exports = {
  assertTestCopyStopped,
  install,
  readState,
  writeState,
};
