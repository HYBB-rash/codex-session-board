import { randomUUID } from "node:crypto";
import {
    closeSync,
    constants,
    fstatSync,
    openSync,
    readFileSync,
    renameSync,
    unlinkSync,
    writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { migratedState } from "../extensions/placement.js";
import { object } from "./operations.js";

export function snapshot(value: unknown) {
    const input = object(value);
    if (!input) return null;
    const { states, sessions, health, projectCatalog, archivedSessions } = input;
    if (
        !object(states) ||
        !Array.isArray(sessions) ||
        !sessions.every((session) => {
            const card = object<"id" | "title">(session);
            return card && typeof card.id === "string" && typeof card.title === "string";
        }) ||
        !object(health) ||
        !object(projectCatalog)
    )
        return null;
    return {
        ...input,
        states,
        sessions: sessions as object[],
        health,
        projectCatalog,
        archivedSessions,
    };
}

export function readOnlySnapshot(value: unknown, capturedAt: number) {
    const current = snapshot(value);
    if (!current) return null;
    const { to_archive: _archive, ...states } = object<"to_archive">(current.states) ?? {};
    const offlineCard = (session: object) => ({
        ...session,
        state: migratedState(object<"state">(session)?.state),
        readOnly: true,
        canOpen: false,
        canArchive: false,
        canRestore: false,
        archiveChecking: false,
        archiveReason: "Codex 窗口离线",
        catalogCurrent: false,
        presence: "cachedUnknown",
    });
    return {
        ...current,
        states,
        health: {
            ...object(current.health),
            error: "Codex 窗口离线，显示此前读取的只读快照",
            lastSuccessAt: capturedAt / 1000,
            scanComplete: false,
            runtimeVerified: false,
            nativeVerified: false,
            connected: false,
            cached: true,
            capturedAt,
        },
        projectCatalog: { ...object(current.projectCatalog), cached: true, pending: false },
        sessions: current.sessions.map(offlineCard),
        ...(Array.isArray(current.archivedSessions) && {
            archivedSessions: current.archivedSessions.flatMap((row: unknown) => {
                const session = object(row);
                return session ? [offlineCard(session)] : [];
            }),
        }),
        offline: true,
    };
}

export function writeCache(directory: string, accountKey: string, value: unknown): void {
    const cachedAt = Date.now();
    const data = readOnlySnapshot(value, cachedAt);
    if (!data) return;
    const destination = join(directory, "board-offline.json");
    const temporary = `${destination}.${randomUUID()}.tmp`;
    const fd = openSync(temporary, "wx", 0o600);
    try {
        writeFileSync(fd, JSON.stringify({ accountKey, cachedAt, data }), { flush: true });
        renameSync(temporary, destination);
    } catch (error) {
        unlinkSync(temporary);
        throw error;
    } finally {
        closeSync(fd);
    }
}

export function readCache(directory: string, accountKey: string | undefined): unknown {
    if (!accountKey) return null;
    let fd: number;
    try {
        fd = openSync(
            join(directory, "board-offline.json"),
            constants.O_RDONLY | constants.O_NOFOLLOW,
        );
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
        throw error;
    }
    try {
        const stat = fstatSync(fd);
        if (
            !stat.isFile() ||
            stat.uid !== process.getuid?.() ||
            stat.mode & 0o077 ||
            stat.size > 64 * 1024 * 1024
        ) {
            throw new Error("看板离线快照权限或大小无效");
        }
        const saved = object<"accountKey" | "cachedAt" | "data">(
            JSON.parse(readFileSync(fd, "utf8")),
        );
        return saved?.accountKey === accountKey && typeof saved.cachedAt === "number"
            ? readOnlySnapshot(saved.data, saved.cachedAt)
            : null;
    } finally {
        closeSync(fd);
    }
}
