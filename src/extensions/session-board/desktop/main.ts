import type { IpcMainEvent } from "electron";
import { hostId as parseHostId } from "./connection.js";
import { type DesktopCommand, desktopWorldSource } from "./world.js";

function parseDesktopCommand(value: unknown): DesktopCommand {
    if (typeof value !== "object" || value === null)
        throw new Error("Invalid board desktop request");
    const command = value as Partial<Omit<Extract<DesktopCommand, { kind: "move" }>, "kind">> & {
        kind?: unknown;
        projectId?: unknown;
        roots?: unknown;
    };
    if (command.kind === "context") return { kind: "context" };
    const { accountId, userId, hostId, threadId, from, to } = command;
    if (typeof accountId !== "string" || typeof userId !== "string")
        throw new Error("Invalid board account");
    const account = { accountId, userId };
    if (command.kind === "settings") return { kind: "settings", ...account };
    if (typeof hostId !== "string" || !parseHostId(hostId)) throw new Error("Invalid board host");
    if (
        command.kind === "new" &&
        (command.projectId === null || typeof command.projectId === "string") &&
        Array.isArray(command.roots) &&
        command.roots.every((root): root is string => typeof root === "string")
    ) {
        return {
            kind: "new",
            ...account,
            hostId,
            projectId: command.projectId,
            roots: command.roots,
        };
    }
    if (typeof threadId !== "string" || !/^[\da-f-]{36}$/i.test(threadId))
        throw new Error("Invalid thread ID");
    if (command.kind === "open") return { kind: "open", ...account, hostId, threadId };
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
