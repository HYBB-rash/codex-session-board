import { mountReplyPreview, type PreviewSession } from "./reply-preview";

interface ReaderPorts {
    sessions(): PreviewSession[];
    request(path: "/api/open" | "/api/preview-reply", body: { id: string }): Promise<unknown>;
    dragSuppressesClick(): boolean;
    savingWithoutMove(): boolean;
    creating(): boolean;
    movingId(): string | undefined;
    announce(message: string, error?: boolean): void;
}

declare global {
    interface Window {
        boardOpenLink?: (url: string) => Promise<void>;
    }
}

export function mountBoardSessionReader(container: HTMLElement, ports: ReaderPorts) {
    let opening = false;
    const sessionById = (id: string) => ports.sessions().find((row) => row.id === id);
    const blocked = (id: string) =>
        ports.dragSuppressesClick() || ports.savingWithoutMove() || ports.movingId() === id;

    async function openSession(id: string): Promise<true | undefined> {
        if (blocked(id) || opening) return;
        if (sessionById(id)?.canOpen === false) {
            ports.announce("主机暂不可用，无法打开会话。", true);
            return;
        }
        opening = true;
        ports.announce("正在打开 Codex 会话…");
        try {
            await ports.request("/api/open", { id });
            ports.announce("已发送会话跳转请求。");
            return true;
        } catch (error) {
            ports.announce(error instanceof Error ? error.message : String(error), true);
            return undefined;
        } finally {
            opening = false;
        }
    }

    const preview = mountReplyPreview(container, {
        read: (id) => ports.request("/api/preview-reply", { id }),
        openSession,
        canDismiss: () => !ports.dragSuppressesClick(),
        canRead: (id) => !blocked(id) && !ports.creating(),
        openLink(url) {
            if (window.boardOpenLink)
                window
                    .boardOpenLink(url)
                    .catch(() => ports.announce("无法打开回复中的链接。", true));
            else window.open(url, "_blank", "noopener,noreferrer");
        },
    });
    return {
        preview,
        openSession,
        get opening() {
            return opening;
        },
        openReply: (session: PreviewSession, focus = false) =>
            preview.open(sessionById(session.id) ?? session, focus),
    };
}
