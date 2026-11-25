import MarkdownIt from "markdown-it";
import { finalAnswer } from "./activity.js";
import type { DesktopConnection, HostConnection, ThreadId, Turn } from "./desktop/connection.js";

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

function verifiedTurn({ data, nextCursor }: { data: Turn[]; nextCursor: string | null }) {
    const [turn] = data;
    if (data.length > 1 || (!turn && nextCursor !== null)) throw new Error("轮次分页无效");
    if (!turn) return;
    if (typeof turn.id !== "string" || !turn.id || turn.id.length > 128 || /\p{Cc}/u.test(turn.id))
        throw new Error("轮次 ID 无效");
    if (!["completed", "inProgress", "interrupted", "failed"].includes(turn.status))
        throw new Error("未知轮次状态");
    return turn;
}

async function threadRunning(
    desktop: DesktopConnection,
    connection: HostConnection,
    id: ThreadId,
): Promise<boolean> {
    const { status } = await desktop.read(connection, id, false);
    if (status.type === "active") return true;
    if (status.type === "idle" || status.type === "notLoaded") return false;
    throw new Error("无法核对当前轮次状态");
}

async function latestAnswer(
    desktop: DesktopConnection,
    connection: HostConnection,
    id: ThreadId,
    initial: Awaited<ReturnType<DesktopConnection["turns"]>>,
    latest: Turn | undefined,
) {
    let page = initial;
    let turn = latest;
    const cursors = new Set<string>();
    while (turn) {
        const text = finalAnswer(turn);
        if (text) return { turnId: turn.id, html: markdown.render(text) };
        if (!page.nextCursor) break;
        if (cursors.has(page.nextCursor)) throw new Error("历史轮次游标重复");
        cursors.add(page.nextCursor);
        page = await desktop.turns(connection, id, "summary", page.nextCursor);
        turn = verifiedTurn(page);
    }
    return null;
}

export async function previewReply(
    desktop: DesktopConnection,
    connection: HostConnection,
    id: ThreadId,
) {
    for (let attempt = 0; attempt < 2; attempt++) {
        const running = await threadRunning(desktop, connection, id);
        const page = await desktop.turns(connection, id, "summary");
        const latest = verifiedTurn(page);
        const answer = await latestAnswer(desktop, connection, id, page, latest);
        const check = verifiedTurn(await desktop.turns(connection, id, "notLoaded"));
        const after = await threadRunning(desktop, connection, id);
        if (signature(latest) === signature(check) && running === after) {
            const inProgress = after || latest?.status === "inProgress";
            return { turnId: answer?.turnId ?? null, html: answer?.html ?? null, inProgress };
        }
    }
    throw new Error("读取期间轮次发生变化，请重试");
}
