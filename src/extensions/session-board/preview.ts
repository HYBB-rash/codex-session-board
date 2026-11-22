import MarkdownIt from "markdown-it";
import { finalAnswer } from "./activity.js";
import type { DesktopConnection, HostConnection, ThreadId, Turn } from "./desktop-connection.js";

const markdown = new MarkdownIt({ html: false }).disable("image");
markdown.validateLink = (input) => {
    try {
        const url = new URL(input);
        return (
            ["http:", "https:"].includes(url.protocol) &&
            Boolean(url.hostname) &&
            !url.username &&
            !url.password
        );
    } catch {
        return false;
    }
};

function signature(turn: Turn | undefined): string {
    return JSON.stringify(turn ? [turn.id, turn.status] : null);
}

export async function previewReply(
    desktop: DesktopConnection,
    connection: HostConnection,
    id: ThreadId,
) {
    for (let attempt = 0; attempt < 2; attempt++) {
        const before = await desktop.read(connection, id, false);
        let page = await desktop.turns(connection, id, "summary");
        const latest = page.data[0];
        const cursors = new Set<string>();
        let answer: { turnId: string; html: string } | null = null;
        for (;;) {
            if (page.data.length > 1 || (!page.data.length && page.nextCursor))
                throw new Error("轮次分页无效");
            const turn = page.data[0];
            const text = finalAnswer(turn);
            if (turn && text) {
                answer = { turnId: turn.id, html: markdown.render(text) };
                break;
            }
            if (!page.nextCursor) break;
            if (cursors.has(page.nextCursor)) throw new Error("历史轮次游标重复");
            cursors.add(page.nextCursor);
            page = await desktop.turns(connection, id, "summary", page.nextCursor);
        }
        const check = await desktop.turns(connection, id, "notLoaded");
        const after = await desktop.read(connection, id, false);
        if (
            signature(latest) === signature(check.data[0]) &&
            before.status.type === after.status.type
        ) {
            const inProgress = after.status.type === "active" || latest?.status === "inProgress";
            return { turnId: answer?.turnId ?? null, html: answer?.html ?? null, inProgress };
        }
    }
    throw new Error("读取期间轮次发生变化，请重试");
}
