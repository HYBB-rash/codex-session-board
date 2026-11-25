import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { sha256ArchiveHeader } from "../asar.js";
import { type Detection, readFuses } from "../detect.js";

export const sentinel = Buffer.from("AGbevlPCksUGKNL8TSn7wGmJEuJsXb2A");
const slotLength = sentinel.length + 2 + 32;

function embeddedDigestOffsets(binary: string): number[] {
    const file = fs.openSync(binary, "r");
    const chunkSize = 1024 * 1024;
    const chunk = Buffer.alloc(chunkSize);
    const offsets: number[] = [];
    try {
        const size = fs.fstatSync(file).size;
        for (let offset = 0; offset < size; offset += chunkSize - slotLength + 1) {
            const length = fs.readSync(file, chunk, 0, chunkSize, offset);
            const candidate = chunk.subarray(0, length);
            let index = candidate.indexOf(sentinel);
            while (index >= 0 && index + slotLength <= length) {
                if (candidate[index + sentinel.length] === 1) offsets.push(offset + index);
                index = candidate.indexOf(sentinel, index + sentinel.length);
            }
        }
        return offsets;
    } finally {
        fs.closeSync(file);
    }
}

export function embeddedFramework(appDirectory: string): string | null {
    const frameworks = path.join(appDirectory, "Contents", "Frameworks");
    let match: string | null = null;
    for (const entry of fs.readdirSync(frameworks, { withFileTypes: true })) {
        if (!entry.name.endsWith(".framework")) continue;
        const name = path.basename(entry.name, ".framework");
        const binary = path.join(frameworks, entry.name, name);
        if (!fs.statSync(binary, { throwIfNoEntry: false })?.isFile()) continue;
        const offsets = embeddedDigestOffsets(binary);
        if (offsets.length === 0) continue;
        if (offsets.length !== 1 || match) throw new Error("multiple enabled ASAR digest slots");
        match = fs.realpathSync(binary);
    }
    return match;
}

interface IntegrityEntry {
    algorithm: string;
    hash: string;
}

export function integrityDigest(manifest: Readonly<Record<string, IntegrityEntry>>): Buffer {
    const hash = createHash("sha256");
    for (const key of Object.keys(manifest).sort()) {
        const entry = manifest[key];
        if (!entry) throw new Error("missing ASAR integrity entry");
        hash.update(key).update(entry.algorithm).update(entry.hash);
    }
    return hash.digest();
}

function validEntry(entry: unknown): entry is IntegrityEntry {
    return (
        typeof entry === "object" &&
        entry !== null &&
        "algorithm" in entry &&
        entry.algorithm === "SHA256" &&
        "hash" in entry &&
        typeof entry.hash === "string" &&
        /^[a-f0-9]{64}$/.test(entry.hash)
    );
}

function readIntegrity(plist: string): Record<string, IntegrityEntry> {
    const output = execFileSync(
        "/usr/bin/plutil",
        ["-extract", "ElectronAsarIntegrity", "json", "-o", "-", plist],
        { encoding: "utf8" },
    );
    const record: unknown = JSON.parse(output);
    if (typeof record !== "object" || !record || Array.isArray(record)) {
        throw new Error("invalid ASAR integrity dictionary");
    }
    const entries = Object.entries(record).map(([key, entry]): [string, IntegrityEntry] => {
        if (!validEntry(entry)) throw new Error(`invalid integrity entry for ${key}`);
        return [key, entry];
    });
    return Object.fromEntries(entries);
}

function readExactly(file: number, length: number, offset: number): Buffer {
    const bytes = Buffer.alloc(length);
    if (fs.readSync(file, bytes, 0, length, offset) !== length) {
        throw new Error("truncated Electron framework");
    }
    return bytes;
}

function segmentSlot(commands: Buffer, offset: number, size: number): number[] {
    const segment = commands.toString("ascii", offset + 8, offset + 20);
    if (segment !== "__DATA_CONST") return [];
    const sectionCount = commands.readUInt32LE(offset + 64);
    if (72 + sectionCount * 80 > size) throw new Error("invalid framework sections");
    const slots: number[] = [];
    for (let section = offset + 72; section < offset + 72 + sectionCount * 80; section += 80) {
        const name = commands.toString("ascii", section, section + 16).replace(/\0+$/, "");
        if (name !== "__asar_integrity") continue;
        if (commands.readBigUInt64LE(section + 40) !== BigInt(slotLength)) {
            throw new Error("unsupported ASAR digest section size");
        }
        slots.push(commands.readUInt32LE(section + 48));
    }
    return slots;
}

function digestSection(file: number): number {
    const header = readExactly(file, 32, 0);
    if (header.readUInt32LE(0) !== 0xfeedfacf || header.readUInt32LE(4) !== 0x0100000c) {
        throw new Error("embedded ASAR integrity requires a thin arm64 framework");
    }
    const commandsSize = header.readUInt32LE(20);
    if (commandsSize > 1024 * 1024) throw new Error("invalid framework load command size");
    const commands = readExactly(file, commandsSize, header.length);
    const offsets: number[] = [];
    for (let offset = 0; offset < commands.length; ) {
        if (offset + 8 > commands.length) throw new Error("truncated framework load command");
        const command = commands.readUInt32LE(offset);
        const size = commands.readUInt32LE(offset + 4);
        if (size < 8 || offset + size > commands.length) {
            throw new Error("invalid framework load command");
        }
        if (command === 0x19 && size >= 72) offsets.push(...segmentSlot(commands, offset, size));
        offset += size;
    }
    const [slot] = offsets;
    if (slot === undefined || offsets.length !== 1) {
        throw new Error("missing or repeated ASAR digest section");
    }
    return slot;
}

export function assertFrameworkDigest(file: number, digest: Buffer): number {
    const offset = digestSection(file);
    const slot = readExactly(file, slotLength, offset);
    if (
        !slot.subarray(0, sentinel.length).equals(sentinel) ||
        slot[sentinel.length] !== 1 ||
        slot[sentinel.length + 1] !== 1 ||
        !slot.subarray(sentinel.length + 2).equals(digest)
    ) {
        throw new Error("enabled framework ASAR digest does not match the signed plist");
    }
    return offset + sentinel.length + 2;
}

interface EmbeddedIntegrity {
    binary: string;
    plist: string;
    key: string;
    digest: Buffer;
}

export function verifyEmbeddedIntegrity(copy: Detection, binary: string): EmbeddedIntegrity {
    const framework = path.join(copy.appDir, "Contents/Frameworks/Codex Framework.framework");
    const expected = fs.realpathSync(path.join(framework, "Codex Framework"));
    if (binary !== expected || !binary.startsWith(`${framework}${path.sep}`)) {
        throw new Error("unsupported Codex framework location");
    }
    const fuses = readFuses(binary)?.flags;
    const required = ["EnableEmbeddedAsarIntegrityValidation", "OnlyLoadAppFromAsar"];
    if (!fuses || !required.every((name) => fuses[name])) {
        throw new Error("the Codex ASAR integrity fuses must remain enabled");
    }
    const plist = path.join(copy.appDir, "Contents/Info.plist");
    const { archivePath, relativeArchive } = copy.archive;
    if (fs.realpathSync(plist) !== plist || fs.realpathSync(archivePath) !== archivePath) {
        throw new Error("ASAR integrity metadata must reside inside the app bundle");
    }
    const key = relativeArchive.replace(/^Contents\//, "");
    const manifest = readIntegrity(plist);
    if (manifest[key]?.hash !== sha256ArchiveHeader(archivePath)) {
        throw new Error("ASAR header does not match the signed plist");
    }
    const digest = integrityDigest(manifest);
    const file = fs.openSync(binary, "r");
    try {
        assertFrameworkDigest(file, digest);
    } finally {
        fs.closeSync(file);
    }
    return { binary, plist, key, digest };
}

export function updateEmbeddedIntegrity(copy: Detection, integrity: EmbeddedIntegrity): void {
    const key = integrity.key.replaceAll(".", "\\.");
    const hash = sha256ArchiveHeader(copy.archive.archivePath);
    execFileSync("/usr/bin/plutil", [
        "-replace",
        `ElectronAsarIntegrity.${key}.hash`,
        "-string",
        hash,
        integrity.plist,
    ]);
    const digest = integrityDigest(readIntegrity(integrity.plist));
    const file = fs.openSync(integrity.binary, "r+");
    try {
        const offset = assertFrameworkDigest(file, integrity.digest);
        if (fs.writeSync(file, digest, 0, digest.length, offset) !== digest.length) {
            throw new Error("could not replace the framework ASAR digest");
        }
    } finally {
        fs.closeSync(file);
    }
}
