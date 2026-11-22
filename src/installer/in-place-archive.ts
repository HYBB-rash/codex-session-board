import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { entryIncludes, patchEntry, sha256File } from "./asar.js";
import { SENTINEL } from "./constants.js";
import { type FileProof, fileProof, sameFileProof } from "./in-place-target.js";

export interface ArchiveTransaction {
    archive: string;
    backup: string;
    staged: string;
    receipt: string;
}

export interface ArchiveProof {
    original: FileProof;
    patched: FileProof;
}

export type ArchivePosition = "original" | "patched" | "unknown";

function sync(item: string): void {
    const file = fs.openSync(item, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    try {
        fs.fsyncSync(file);
    } finally {
        fs.closeSync(file);
    }
}

export function syncDirectory(directory: string): void {
    sync(directory);
}

export function presentFile(item: string): boolean {
    const file = fs.lstatSync(item, { throwIfNoEntry: false });
    if (!file) return false;
    if (!file.isFile() || file.nlink !== 1)
        throw new Error(`安装事务路径不是独立的常规文件：${item}`);
    return true;
}

function copyAttributes(source: string, destination: string, attributesOnly = false): void {
    execFileSync(
        "/usr/bin/cp",
        [
            "--preserve=all",
            attributesOnly ? "--attributes-only" : "--reflink=auto",
            "--",
            source,
            destination,
        ],
        { env: { PATH: "/usr/bin:/bin", LC_ALL: "C" }, stdio: ["ignore", "pipe", "pipe"] },
    );
    sync(destination);
}

function discard(file: string): void {
    if (!presentFile(file)) return;
    fs.unlinkSync(file);
    syncDirectory(path.dirname(file));
}

export function writeInstallationReceipt<T extends ArchiveProof>(receipt: string, value: T): void {
    const directory = path.dirname(receipt);
    const next = path.join(directory, `.receipt-${randomUUID()}`);
    try {
        fs.writeFileSync(next, `${JSON.stringify(value, null, 2)}\n`, {
            flag: "wx",
            mode: 0o644,
            flush: true,
        });
        fs.renameSync(next, receipt);
        syncDirectory(directory);
    } finally {
        discard(next);
    }
}

export function prepareArchive(
    transaction: ArchiveTransaction,
    entry: string,
    original: FileProof,
    loader: string,
): ArchiveProof {
    const { archive, backup, staged } = transaction;
    if (presentFile(backup) || presentFile(staged))
        throw new Error("有未清理的归档事务，请先检查安装状态");
    if (!sameFileProof(fileProof(archive), original))
        throw new Error("Codex 启动归档在预检后发生变化");
    if (entryIncludes(archive, entry, Buffer.from(SENTINEL)))
        throw new Error("Codex 启动归档已有看板 Loader");
    copyAttributes(archive, backup);
    syncDirectory(path.dirname(backup));
    if (!sameFileProof(fileProof(backup), original))
        throw new Error("备份的内容或文件元数据与原件不同");
    copyAttributes(backup, staged);
    const prefix = Buffer.from(loader);
    patchEntry(staged, entry, (current) => Buffer.concat([prefix, current]));
    copyAttributes(backup, staged, true);
    syncDirectory(path.dirname(staged));
    const patched = fileProof(staged);
    if (!sameFileProof({ ...patched, sha256: original.sha256 }, original))
        throw new Error("补丁归档的文件元数据与原件不同");
    return { original, patched };
}

export function archivePosition(
    { archive }: ArchiveTransaction,
    { original, patched }: ArchiveProof,
): ArchivePosition {
    if (!presentFile(archive)) return "unknown";
    const actual = fileProof(archive);
    if (sameFileProof(actual, patched)) return "patched";
    if (sameFileProof(actual, original)) return "original";
    return "unknown";
}

function replaceArchive(
    transaction: ArchiveTransaction,
    proofs: ArchiveProof,
    replacing: "original" | "patched",
    replacement: FileProof,
    assertStopped: () => void,
): void {
    const { archive, staged } = transaction;
    if (!presentFile(staged) || !sameFileProof(fileProof(staged), replacement))
        throw new Error("待安装归档的内容或元数据无法核对");
    if (archivePosition(transaction, proofs) !== replacing)
        throw new Error("Codex 启动归档已变化，已保留恢复材料");
    assertStopped();
    fs.renameSync(staged, archive);
    syncDirectory(path.dirname(archive));
}

export function commitArchive(
    transaction: ArchiveTransaction,
    proofs: ArchiveProof,
    assertStopped: () => void,
): void {
    if (archivePosition(transaction, proofs) === "patched") return;
    replaceArchive(transaction, proofs, "original", proofs.patched, assertStopped);
}

export function restoreArchive(
    transaction: ArchiveTransaction,
    proofs: ArchiveProof,
    assertStopped: () => void,
): void {
    const position = archivePosition(transaction, proofs);
    if (position === "original") return;
    if (position !== "patched") throw new Error("Codex 启动归档已变化，已保留恢复材料");
    if (
        !presentFile(transaction.backup) ||
        !sameFileProof(fileProof(transaction.backup), proofs.original)
    )
        throw new Error("原始归档的备份无法核对");
    discard(transaction.staged);
    copyAttributes(transaction.backup, transaction.staged);
    replaceArchive(transaction, proofs, "patched", proofs.original, assertStopped);
}

export function cleanArchiveStage(transaction: ArchiveTransaction): void {
    discard(transaction.staged);
    discard(`${transaction.staged}.codex-enhancer-next`);
}

export function discardArchiveBackup(transaction: ArchiveTransaction, expected: FileProof): void {
    if (presentFile(transaction.backup) && sha256File(transaction.backup) !== expected.sha256)
        throw new Error("备份文件已变化，已保留恢复材料");
    discard(transaction.backup);
}
