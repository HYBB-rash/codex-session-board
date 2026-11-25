import { randomUUID } from "node:crypto";
import {
    closeSync,
    constants,
    fstatSync,
    linkSync,
    mkdirSync,
    openSync,
    readFileSync,
    unlinkSync,
    writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { readServiceIdentity, type ServiceIdentity } from "./identity.js";
import { processStartIdentity, processStartTime } from "./process.js";

export interface Receipt {
    readonly app: "codex-session-board";
    readonly url: string;
    readonly clientToken: string;
    readonly instance: string;
    readonly pid: number;
    readonly processStart?: string;
    readonly service?: ServiceIdentity;
}

interface ReceiptFile {
    readonly receipt: Receipt;
    readonly modifiedAt: number;
}

const pathFor = (stateDir: string) => join(stateDir, "board-service.json");

function validLoopbackUrl(url: string): boolean {
    const match = /^http:\/\/127\.0\.0\.1(?::([1-9]\d{0,4}))?\/$/.exec(url);
    return match !== null && Number(match[1] ?? 80) <= 65535;
}

function readReceiptFile(stateDir: string): ReceiptFile | null {
    let descriptor: number;
    try {
        descriptor = openSync(pathFor(stateDir), constants.O_RDONLY | constants.O_NOFOLLOW);
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
        throw error;
    }
    try {
        const stat = fstatSync(descriptor);
        if (
            !stat.isFile() ||
            stat.uid !== process.getuid?.() ||
            stat.mode & 0o077 ||
            stat.size > 4096
        ) {
            throw new Error("看板服务凭据的所有者或权限不符合要求");
        }
        const receipt = JSON.parse(readFileSync(descriptor, "utf8")) as Partial<Receipt> | null;
        if (
            receipt?.app !== "codex-session-board" ||
            typeof receipt.url !== "string" ||
            !validLoopbackUrl(receipt.url) ||
            typeof receipt.clientToken !== "string" ||
            !/^[\da-f]{64}$/.test(receipt.clientToken) ||
            typeof receipt.instance !== "string" ||
            typeof receipt.pid !== "number" ||
            !Number.isSafeInteger(receipt.pid) ||
            receipt.pid <= 0 ||
            (receipt.processStart !== undefined &&
                (typeof receipt.processStart !== "string" ||
                    !/^(?:darwin:\d+|linux:[\da-f-]{36}:pid:\[\d+\]:\d+)$/.test(
                        receipt.processStart,
                    ))) ||
            (receipt.service !== undefined && !readServiceIdentity(receipt.service))
        ) {
            throw new Error("看板服务凭据格式无效");
        }
        return { receipt: receipt as Receipt, modifiedAt: stat.mtimeMs };
    } finally {
        closeSync(descriptor);
    }
}

export function readReceipt(stateDir: string): Receipt | null {
    return readReceiptFile(stateDir)?.receipt ?? null;
}

export function releaseReceipt(stateDir: string, receipt: Receipt): void {
    if (readReceipt(stateDir)?.instance === receipt.instance) unlinkSync(pathFor(stateDir));
}

function processAlive(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ESRCH") return false;
        throw error;
    }
}

function hasLiveOwner({ receipt, modifiedAt }: ReceiptFile): boolean {
    if (!processAlive(receipt.pid)) return false;
    if (receipt.processStart !== undefined) {
        const current = processStartIdentity(receipt.pid);
        return current === null || current === receipt.processStart;
    }
    const current = processStartTime(receipt.pid);
    const legacyClockToleranceMs = 2000;
    return current === null || current <= modifiedAt + legacyClockToleranceMs;
}

export function readActiveReceipt(stateDir: string): Receipt | null {
    const file = readReceiptFile(stateDir);
    return file && hasLiveOwner(file) ? file.receipt : null;
}

export function requireInactiveReceipt(stateDir: string): Receipt | null {
    const previous = readReceiptFile(stateDir);
    if (previous && hasLiveOwner(previous)) throw new Error("已有看板服务实例");
    return previous?.receipt ?? null;
}

export function writeReceipt(stateDir: string, receipt: Receipt): void {
    mkdirSync(stateDir, { recursive: true, mode: 0o700 });
    const previous = requireInactiveReceipt(stateDir);
    if (previous) releaseReceipt(stateDir, previous);
    const destination = pathFor(stateDir);
    const temporary = `${destination}.${randomUUID()}.tmp`;
    writeFileSync(temporary, JSON.stringify(receipt), { flag: "wx", mode: 0o600, flush: true });
    try {
        linkSync(temporary, destination);
    } finally {
        unlinkSync(temporary);
    }
}
