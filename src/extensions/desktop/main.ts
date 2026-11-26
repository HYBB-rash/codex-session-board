import { realpath } from "node:fs/promises";
import { isAbsolute } from "node:path";
import type { IpcMainEvent } from "electron";
import { hostId as parseHostId } from "./connection.js";
import { type DesktopCommand, type DesktopRequest, desktopWorldSource } from "./world.js";

function parseNewTarget(projectId: unknown, roots: unknown) {
    if (projectId !== null && typeof projectId !== "string") return;
    if (!Array.isArray(roots) || !roots.every((root): root is string => typeof root === "string"))
        return;
    return { projectId, roots };
}

function validThreadId(value: unknown): value is string {
    return typeof value === "string" && /^[\da-f-]{36}$/i.test(value);
}

function parseDraftCommand(draftId: unknown, account: { accountId: string; userId: string }) {
    if (typeof draftId !== "string" || !/^client-new-thread:[\da-f-]{36}$/i.test(draftId))
        throw new Error("Invalid board draft ID");
    return { kind: "draft" as const, ...account, draftId };
}

function parseLocalProjectRoot(cwd: unknown, roots: unknown): DesktopRequest {
    if (
        typeof cwd !== "string" ||
        !isAbsolute(cwd) ||
        !Array.isArray(roots) ||
        !roots.every((root): root is string => typeof root === "string" && isAbsolute(root))
    )
        throw new Error("Invalid local project directory");
    return { kind: "local-project-root", cwd, roots };
}

async function resolvedDirectory(path: string) {
    try {
        return await realpath(path);
    } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code === "ENOENT" || code === "ENOTDIR") return undefined;
        throw error;
    }
}

export async function matchesLocalProjectRoot(cwd: string, roots: readonly string[]) {
    const directory = await resolvedDirectory(cwd);
    if (directory === undefined) return false;
    return (await Promise.all(roots.map(resolvedDirectory))).includes(directory);
}

function parseDesktopCommand(value: unknown): DesktopRequest {
    if (typeof value !== "object" || value === null)
        throw new Error("Invalid board desktop request");
    const command = value as Partial<Omit<Extract<DesktopCommand, { kind: "move" }>, "kind">> & {
        kind?: unknown;
        projectId?: unknown;
        roots?: unknown;
        besideBoard?: unknown;
        draftId?: unknown;
        cwd?: unknown;
    };
    switch (command.kind) {
        case "context":
            return { kind: "context" };
        case "local-project-root":
            return parseLocalProjectRoot(command.cwd, command.roots);
    }
    const { accountId, userId, hostId, threadId, from, to } = command;
    if (typeof accountId !== "string" || typeof userId !== "string")
        throw new Error("Invalid board account");
    const account = { accountId, userId };
    switch (command.kind) {
        case "settings":
            return { kind: "settings", ...account };
        case "draft":
            return parseDraftCommand(command.draftId, account);
    }
    if (typeof hostId !== "string" || !parseHostId(hostId)) throw new Error("Invalid board host");
    switch (command.kind) {
        case "data-home":
            return { kind: "data-home", ...account, hostId };
        case "new":
            return parseNewCommand(command.projectId, command.roots, { ...account, hostId });
    }
    if (!validThreadId(threadId)) throw new Error("Invalid thread ID");
    if (command.kind === "open")
        return {
            kind: "open",
            ...account,
            hostId,
            threadId,
            besideBoard: command.besideBoard === true,
        };
    if (
        command.kind !== "move" ||
        (from !== null && typeof from !== "string") ||
        typeof to !== "string"
    )
        throw new Error("Invalid board desktop request");
    return { kind: "move", ...account, hostId, threadId, from, to };
}

function parseNewCommand(
    projectId: unknown,
    roots: unknown,
    owner: { accountId: string; userId: string; hostId: string },
): DesktopCommand {
    const target = parseNewTarget(projectId, roots);
    if (!target) throw new Error("Invalid project target");
    return { kind: "new", ...owner, ...target };
}

export function boardDesktopRequest(event: IpcMainEvent, value: unknown): Promise<unknown> {
    const command = parseDesktopCommand(value);
    if (command.kind === "local-project-root")
        return matchesLocalProjectRoot(command.cwd, command.roots);
    return event.sender.executeJavaScript(desktopWorldSource(command));
}
