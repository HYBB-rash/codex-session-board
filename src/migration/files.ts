import { createHash } from "node:crypto";
import {
    closeSync,
    constants,
    lstatSync,
    openSync,
    readFileSync,
    readSync,
    realpathSync,
    writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

export function inspect(path: string) {
    try {
        const stat = lstatSync(path);
        if (stat.isSymbolicLink()) throw new Error(`迁移路径包含符号链接：${path}`);
        return stat;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
        throw error;
    }
}

export function directory(path: string): boolean {
    const stat = inspect(path);
    if (stat && !stat.isDirectory()) throw new Error(`迁移路径不是目录：${path}`);
    return stat !== undefined;
}

export function regularFile(path: string): boolean {
    const stat = inspect(path);
    if (stat && !stat.isFile()) throw new Error(`迁移路径不是普通文件：${path}`);
    return stat !== undefined;
}

export function canonicalLeaf(path: string): string {
    const absolute = resolve(path);
    const parent = dirname(absolute);
    if (absolute === parent) return absolute;
    try {
        return join(realpathSync(parent), basename(absolute));
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
        return join(canonicalLeaf(parent), basename(absolute));
    }
}

export function canonicalSource(path: string): string {
    const absolute = resolve(path);
    try {
        return realpathSync(absolute);
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
        const supplied = canonicalLeaf(absolute);
        inspect(supplied);
        return supplied;
    }
}

export function assertAbsent(path: string): void {
    if (inspect(path)) throw new Error(`迁移目标已经存在，不会覆盖：${path}`);
}

export function readRegular(path: string): Buffer {
    if (!regularFile(path)) throw new Error(`迁移源文件不存在：${path}`);
    return readFileSync(path, { flag: constants.O_RDONLY | constants.O_NOFOLLOW });
}

export function writePrivate(path: string, contents: Buffer | string): void {
    writeFileSync(path, contents, { flag: "wx", mode: 0o600, flush: true });
}

export function digest(path: string): string {
    if (!regularFile(path)) throw new Error(`迁移文件不存在：${path}`);
    const descriptor = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
        const hash = createHash("sha256");
        const buffer = Buffer.alloc(256 * 1024);
        for (
            let count = readSync(descriptor, buffer);
            count;
            count = readSync(descriptor, buffer)
        ) {
            hash.update(buffer.subarray(0, count));
        }
        return hash.digest("hex");
    } finally {
        closeSync(descriptor);
    }
}
