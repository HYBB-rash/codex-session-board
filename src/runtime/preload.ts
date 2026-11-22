/// <reference lib="dom" />

import { ipcRenderer } from "electron";
import type { BoardLedger } from "../extensions/session-board/archive.js";
import { BoardService } from "../extensions/session-board/board-service.js";
import {
    DesktopConnection,
    hostId,
    type ThreadId,
} from "../extensions/session-board/desktop-connection.js";
import type { CardRecord } from "../extensions/session-board/records.js";
import { readSettings } from "../extensions/session-board/settings.js";
import type { BoardRelayRequest } from "./board-broker.js";
import { startUiHost } from "./host.js";

function proof(stage: string, extra: Record<string, unknown> = {}): void {
    ipcRenderer.send("codex-enhancer:proof", { stage, href: location.href, ...extra });
}

export type MountBoard = (
    panel: HTMLElement,
    request: (path: string, body?: object) => Promise<unknown>,
    close: () => void,
) => () => void;

function observeDesktop(desktop: DesktopConnection, board: BoardService): void {
    ipcRenderer.on("codex_desktop:message-for-view", (_event, value) => {
        const packet = desktop.receive(value);
        const host = packet?.hostId && hostId(packet.hostId);
        const thread = packet?.params?.threadId;
        if (
            packet?.type === "mcp-notification" &&
            host &&
            thread &&
            /^(turn\/(started|completed)|thread\/(status\/changed|archived|unarchived)|item\/completed)$/.test(
                packet.method ?? "",
            )
        ) {
            board.notice(host, thread as ThreadId);
        }
    });
    const timer = window.setInterval(() => {
        if (document.querySelector("[data-app-shell-workspace]")) {
            void board
                .refresh()
                .catch((error: unknown) =>
                    proof("board-observation-failed", { error: String(error) }),
                );
        }
    }, 4000);
    window.addEventListener("pagehide", () => window.clearInterval(timer), { once: true });
}

function connectBoard(desktop: DesktopConnection, closePage: () => void): BoardService {
    return new BoardService(desktop, {
        async load(account) {
            return (await ipcRenderer.invoke("codex-enhancer:board-records", account)) as Record<
                string,
                CardRecord
            >;
        },
        async save(update) {
            await ipcRenderer.invoke("codex-enhancer:board-records", update);
        },
        async settleTitle(resolution) {
            return (
                (await ipcRenderer.invoke("codex-enhancer:board-title-result", resolution)) === true
            );
        },
        async loadLedger(account) {
            return (await ipcRenderer.invoke(
                "codex-enhancer:board-ledger",
                account,
            )) as BoardLedger;
        },
        async writeLedger(account, ledger) {
            await ipcRenderer.invoke("codex-enhancer:board-ledger", account, ledger);
        },
        async settings(account) {
            return readSettings(await ipcRenderer.invoke("codex-enhancer:board-config", account));
        },
        closePage,
        authority(account) {
            ipcRenderer.send("codex-enhancer:board-authority", account);
        },
        updateStatus(start) {
            return ipcRenderer.invoke("codex-enhancer:board-update", start);
        },
        proof,
    });
}

function relay(board: BoardService): void {
    ipcRenderer.on("codex-enhancer:board-relay", (_event, value) => {
        const { id, path, body } = value as BoardRelayRequest;
        void board.request(path, body).then(
            (result) => ipcRenderer.send("codex-enhancer:board-result", { id, value: result }),
            (error: unknown) =>
                ipcRenderer.send("codex-enhancer:board-result", {
                    id,
                    error: error instanceof Error ? error.message : String(error),
                }),
        );
    });
}

function desktopBoard(mountBoard: MountBoard): void {
    const desktop = new DesktopConnection({
        bootstrap: () => ipcRenderer.sendSync("codex_desktop:get-initial-sidebar-bootstrap"),
        send: (packet) => ipcRenderer.invoke("codex_desktop:message-from-view", packet),
        sidebar: (command) => ipcRenderer.invoke("codex-enhancer:board-desktop", command),
    });
    let closePage: (() => void) | undefined;
    const board = connectBoard(desktop, () => closePage?.());
    relay(board);
    observeDesktop(desktop, board);
    startUiHost({
        fetchState: () => ipcRenderer.invoke("codex-enhancer:ui-items"),
        subscribe(listener) {
            ipcRenderer.on("codex-enhancer:ui-items", (_event, payload) => listener(payload));
        },
        proof,
        mountPage(id, panel, close) {
            if (id !== "session-board") return;
            closePage = close;
            const unmount = mountBoard(panel, board.request.bind(board), close);
            return () => {
                closePage = undefined;
                unmount();
            };
        },
    });
}

export function start(mountBoard: MountBoard): void {
    proof("preload-entered", { protocol: location.protocol, top: window.top === window });
    if (
        location.origin !== "app://-" ||
        location.pathname !== "/index.html" ||
        window.top !== window
    )
        return;
    try {
        desktopBoard(mountBoard);
    } catch (error) {
        proof("preload-failed", {
            error: error instanceof Error ? (error.stack ?? error.message) : String(error),
        });
    }
}
