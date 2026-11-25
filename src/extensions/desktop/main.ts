import type { IpcMainEvent } from "electron";
import { hostId as parseHostId } from "./connection.js";
import { type DesktopCommand, desktopWorldSource } from "./world.js";

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

function parseDesktopCommand(value: unknown): DesktopCommand {
    if (typeof value !== "object" || value === null)
        throw new Error("Invalid board desktop request");
    const command = value as Partial<Omit<Extract<DesktopCommand, { kind: "move" }>, "kind">> & {
        kind?: unknown;
        projectId?: unknown;
        roots?: unknown;
        besideBoard?: unknown;
        draftId?: unknown;
    };
    if (command.kind === "context") return { kind: "context" };
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
    const target =
        command.kind === "new" ? parseNewTarget(command.projectId, command.roots) : undefined;
    if (target) return { kind: "new", ...account, hostId, ...target };
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

export function boardDesktopRequest(event: IpcMainEvent, value: unknown): Promise<unknown> {
    const command = parseDesktopCommand(value);
    return event.sender.executeJavaScript(desktopWorldSource(command));
}
