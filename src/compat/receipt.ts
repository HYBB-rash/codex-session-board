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

export interface Receipt {
    readonly app: "codex-session-board";
    readonly url: string;
    readonly clientToken: string;
    readonly instance: string;
    readonly pid: number;
}

const pathFor = (stateDir: string) => join(stateDir, "board-service.json");

export function readReceipt(stateDir: string): Receipt | null {
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
            !/^http:\/\/127\.0\.0\.1:[1-9]\d{0,4}\/$/.test(receipt.url) ||
            typeof receipt.clientToken !== "string" ||
            !/^[\da-f]{64}$/.test(receipt.clientToken) ||
            typeof receipt.instance !== "string" ||
            typeof receipt.pid !== "number" ||
            !Number.isSafeInteger(receipt.pid) ||
            receipt.pid <= 0
        ) {
            throw new Error("看板服务凭据格式无效");
        }
        return receipt as Receipt;
    } finally {
        closeSync(descriptor);
    }
}

export function releaseReceipt(stateDir: string, receipt: Receipt): void {
    if (readReceipt(stateDir)?.instance === receipt.instance) unlinkSync(pathFor(stateDir));
}

function alive(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ESRCH") return false;
        throw error;
    }
}

export function writeReceipt(stateDir: string, receipt: Receipt): void {
    mkdirSync(stateDir, { recursive: true, mode: 0o700 });
    const previous = readReceipt(stateDir);
    if (previous) {
        if (alive(previous.pid)) throw new Error("已有看板服务实例");
        releaseReceipt(stateDir, previous);
    }
    const destination = pathFor(stateDir);
    const temporary = `${destination}.${randomUUID()}.tmp`;
    writeFileSync(temporary, JSON.stringify(receipt), { flag: "wx", mode: 0o600, flush: true });
    try {
        linkSync(temporary, destination);
    } finally {
        unlinkSync(temporary);
    }
}
