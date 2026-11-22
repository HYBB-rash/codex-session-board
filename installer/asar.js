"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");

const BLOCK_SIZE = 4 * 1024 * 1024;

function integrityFor(buffer) {
  const blocks = [];
  for (let offset = 0; offset < buffer.length; offset += BLOCK_SIZE) {
    blocks.push(
      crypto.createHash("sha256").update(buffer.subarray(offset, Math.min(offset + BLOCK_SIZE, buffer.length))).digest("hex"),
    );
  }
  return {
    algorithm: "SHA256",
    hash: crypto.createHash("sha256").update(buffer).digest("hex"),
    blockSize: BLOCK_SIZE,
    blocks,
  };
}

function encodeHeader(headerObject) {
  const json = Buffer.from(JSON.stringify(headerObject));
  const jsonLength = json.length;
  const payloadSize = (4 + jsonLength + 3) & ~3;
  const headerPickleLength = 4 + payloadSize;
  const header = Buffer.alloc(8 + headerPickleLength);
  header.writeUInt32LE(4, 0);
  header.writeUInt32LE(headerPickleLength, 4);
  header.writeUInt32LE(payloadSize, 8);
  header.writeUInt32LE(jsonLength, 12);
  json.copy(header, 16);
  return header;
}

function readHeaderFromFd(fd) {
  const prefix = Buffer.alloc(16);
  if (fs.readSync(fd, prefix, 0, 16, 0) !== 16) {
    throw new Error("asar header is shorter than 16 bytes");
  }
  if (prefix.readUInt32LE(0) !== 4) {
    throw new Error("asar size pickle is not a uint32");
  }
  const headerPickleLength = prefix.readUInt32LE(4);
  const payloadSize = prefix.readUInt32LE(8);
  const jsonLength = prefix.readUInt32LE(12);
  if (headerPickleLength !== 4 + payloadSize) {
    throw new Error("asar header pickle size does not match its payload");
  }
  if (payloadSize !== ((4 + jsonLength + 3) & ~3)) {
    throw new Error("asar header string is not aligned");
  }
  const jsonBuffer = Buffer.alloc(jsonLength);
  if (fs.readSync(fd, jsonBuffer, 0, jsonLength, 16) !== jsonLength) {
    throw new Error("asar header json was truncated");
  }
  return {
    header: JSON.parse(jsonBuffer.toString("utf8")),
    dataStart: 8 + headerPickleLength,
  };
}

function openArchive(archivePath) {
  const fd = fs.openSync(archivePath, "r");
  try {
    const parsed = readHeaderFromFd(fd);
    const size = fs.fstatSync(fd).size;
    return { fd, size, ...parsed };
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
}

function lookup(header, entryPath) {
  const parts = entryPath.split("/").filter(Boolean);
  let node = header;
  for (const part of parts) {
    node = node.files?.[part];
    if (!node) return null;
  }
  return node;
}

function readEntry(archive, entryPath) {
  const node = lookup(archive.header, entryPath);
  if (!node || node.files) {
    throw new Error(`asar entry is not a file: ${entryPath}`);
  }
  const size = node.size;
  const offset = Number(node.offset);
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(size)) {
    throw new Error(`asar entry has an unreadable offset: ${entryPath}`);
  }
  const data = Buffer.alloc(size);
  const position = archive.dataStart + offset;
  if (fs.readSync(archive.fd, data, 0, size, position) !== size) {
    throw new Error(`asar entry was truncated: ${entryPath}`);
  }
  return data;
}

function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  const fd = fs.openSync(filePath, "r");
  try {
    const buffer = Buffer.alloc(1024 * 1024);
    let position = 0;
    while (true) {
      const read = fs.readSync(fd, buffer, 0, buffer.length, position);
      if (read === 0) break;
      hash.update(buffer.subarray(0, read));
      position += read;
    }
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest("hex");
}

function createArchive(filePath, files) {
  let data = Buffer.alloc(0);
  const root = { files: {} };
  for (const file of files) {
    const bytes = Buffer.from(file.data);
    const parts = file.path.split("/");
    let directory = root;
    for (const part of parts.slice(0, -1)) {
      directory.files[part] ??= { files: {} };
      directory = directory.files[part];
    }
    directory.files[parts.at(-1)] = {
      offset: String(data.length),
      size: bytes.length,
      integrity: integrityFor(bytes),
    };
    data = Buffer.concat([data, bytes]);
  }
  const header = encodeHeader(root);
  fs.writeFileSync(filePath, Buffer.concat([header, data]));
}

function patchEntry(archivePath, entryPath, transform) {
  const archive = openArchive(archivePath);
  try {
    const node = lookup(archive.header, entryPath);
    if (!node || node.files) throw new Error(`asar entry is not a file: ${entryPath}`);
    const current = readEntry(archive, entryPath);
    const next = Buffer.from(transform(current));
    if (next.equals(current)) return { changed: false, bytes: next };

    const dataSize = archive.size - archive.dataStart;
    node.offset = String(dataSize);
    node.size = next.length;
    node.integrity = integrityFor(next);
    const header = encodeHeader(archive.header);
    const nextPath = `${archivePath}.codex-enhancer-next`;
    const output = fs.openSync(nextPath, "w", 0o644);
    try {
      fs.writeSync(output, header);
      const buffer = Buffer.alloc(1024 * 1024);
      let remaining = dataSize;
      let position = archive.dataStart;
      while (remaining > 0) {
        const read = fs.readSync(archive.fd, buffer, 0, Math.min(buffer.length, remaining), position);
        if (read <= 0) throw new Error("asar data section ended early");
        fs.writeSync(output, buffer, 0, read);
        position += read;
        remaining -= read;
      }
      fs.writeSync(output, next);
    } catch (error) {
      fs.closeSync(output);
      fs.rmSync(nextPath, { force: true });
      throw error;
    }
    fs.closeSync(output);
    fs.renameSync(nextPath, archivePath);
    return { changed: true, bytes: next };
  } finally {
    fs.closeSync(archive.fd);
  }
}

function listAsarFiles(resourcesDir) {
  return fs
    .readdirSync(resourcesDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".asar"))
    .map((entry) => entry.name);
}

module.exports = {
  BLOCK_SIZE,
  createArchive,
  encodeHeader,
  integrityFor,
  listAsarFiles,
  lookup,
  openArchive,
  patchEntry,
  readEntry,
  sha256File,
};
