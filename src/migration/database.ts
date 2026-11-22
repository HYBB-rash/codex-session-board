import { spawnSync } from "node:child_process";
import { closeSync, constants, copyFileSync, mkdtempSync, openSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { backup, DatabaseSync } from "node:sqlite";
import { readRegular, regularFile, writePrivate } from "./files.js";

export const databaseName = "status.sqlite3";
const sidecars = ["-wal", "-shm", "-journal"] as const;

export function existingDatabase(source: string): boolean {
    const exists = regularFile(source);
    for (const suffix of sidecars) regularFile(`${source}${suffix}`);
    return exists;
}

function validate(database: DatabaseSync): void {
    const rows = database.prepare("PRAGMA quick_check").all();
    if (rows.length !== 1 || Object.values(rows[0] ?? {})[0] !== "ok") {
        throw new Error("迁移状态库完整性核对失败");
    }
}

async function onlineBackup(source: string, target: string): Promise<void> {
    const database = new DatabaseSync(source, { readOnly: true });
    try {
        database.exec("PRAGMA busy_timeout=3000; PRAGMA trusted_schema=OFF");
        database.prepare("PRAGMA schema_version").get();
        await backup(database, target, { rate: 128 });
    } finally {
        database.close();
    }
    const copy = new DatabaseSync(target);
    try {
        copy.exec("PRAGMA journal_mode=DELETE; PRAGMA trusted_schema=OFF");
        validate(copy);
    } finally {
        copy.close();
    }
}

function sharedLock(descriptor: number): void {
    if (process.platform !== "linux")
        throw new Error("当前平台无法锁定并恢复热回滚日志，需要干净的 SQLite 副本");
    const result = spawnSync("flock", ["--fcntl", "--shared", "--nonblock", "3"], {
        stdio: ["ignore", "pipe", "pipe", descriptor],
    });
    if (result.error || result.status !== 0) {
        throw new Error("无法锁定热回滚日志来源，可能仍有写入者或系统缺少 flock --fcntl", {
            cause: result.error,
        });
    }
}

function recoverPrivately(source: string, recovered: string): void {
    const descriptor = openSync(source, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
        sharedLock(descriptor);
        if (regularFile(`${source}-wal`))
            throw new Error("WAL 和热回滚日志同时存在，无法确认源状态");
        copyFileSync(source, recovered, constants.COPYFILE_EXCL);
        writePrivate(`${recovered}-journal`, readRegular(`${source}-journal`));
    } finally {
        closeSync(descriptor);
    }
    const database = new DatabaseSync(recovered);
    try {
        database.exec("PRAGMA trusted_schema=OFF");
        validate(database);
    } finally {
        database.close();
    }
}

export async function snapshotDatabase(source: string, target: string): Promise<void> {
    writePrivate(target, Buffer.alloc(0));
    try {
        await onlineBackup(source, target);
    } catch (error) {
        if ((error as { errcode?: unknown }).errcode !== 776) throw error;
        const recovery = mkdtempSync(join(dirname(target), ".hot-rollback-"));
        try {
            const recovered = join(recovery, databaseName);
            recoverPrivately(source, recovered);
            await onlineBackup(recovered, target);
        } finally {
            rmSync(recovery, { recursive: true, force: true });
        }
    }
}
