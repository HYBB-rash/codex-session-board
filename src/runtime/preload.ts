/// <reference lib="dom" />

import { ipcRenderer } from "electron";
import type { BoardLedger } from "../extensions/archive.js";
import {
    createDesktopConnection,
    type DesktopConnection,
    hostId,
    type ThreadId,
} from "../extensions/desktop/connection.js";
import type { LoadedBoardRecords } from "../extensions/records.js";
import { type BoardService, type Card, createBoardService } from "../extensions/service.js";
import { readSettings } from "../extensions/settings.js";
import type { BoardRelayRequest } from "./board/broker.js";
import { startUiHost } from "./host.js";

function proof(stage: string, extra: Record<string, unknown> = {}): void {
    ipcRenderer.send("codex-enhancer:proof", { stage, href: location.href, ...extra });
}

export type MountBoard = (
    panel: HTMLElement,
    request: (path: string, body?: object) => Promise<unknown>,
    close: () => void,
    conversation: {
        open(id: string): Promise<void>;
        setVisible(visible: boolean): void;
    },
) => () => void;

export type MountTasks = (container: HTMLElement, board: BoardService) => () => void;

function observeDesktop(desktop: DesktopConnection, board: BoardService): void {
    const receive = (data: unknown) => {
        const packet = desktop.receive(data);
        const host = packet?.hostId && hostId(packet.hostId);
        const thread = packet?.params?.threadId;
        const watch = packet?.params?.watchId;
        const paths = packet?.params?.changedPaths;
        if (
            host &&
            packet?.type === "codex-app-server-connection-changed" &&
            packet.state === "connected"
        )
            board.archiveHostReconnected(host);
        if (
            host &&
            packet?.type === "mcp-notification" &&
            packet.method === "fs/changed" &&
            typeof watch === "string" &&
            Array.isArray(paths) &&
            board.taskChanged(host, watch, paths)
        ) {
            void board
                .refresh()
                .catch((error: unknown) =>
                    proof("board-observation-failed", { error: String(error) }),
                );
        }
        if (
            packet?.type === "mcp-notification" &&
            host &&
            typeof thread === "string" &&
            typeof packet.method === "string" &&
            /^(turn\/(started|completed)|thread\/(status\/changed|archived|unarchived)|item\/completed)$/.test(
                packet.method,
            )
        ) {
            board.notice(host, thread as ThreadId);
        }
    };
    window.addEventListener("message", ({ data, origin, source }) => {
        if (source === window && origin === location.origin) receive(data);
    });
    const nativeChannel = "codex_desktop:message-for-view";
    const receiveNative = (_event: unknown, data: unknown) => receive(data);
    ipcRenderer.on(nativeChannel, receiveNative);
    const timer = window.setInterval(() => {
        if (document.querySelector("[data-app-shell-workspace]")) {
            void board
                .refresh()
                .catch((error: unknown) =>
                    proof("board-observation-failed", { error: String(error) }),
                );
        }
    }, 4000);
    window.addEventListener(
        "pagehide",
        () => {
            window.clearInterval(timer);
            ipcRenderer.removeListener(nativeChannel, receiveNative);
        },
        { once: true },
    );
}

function connectBoard(desktop: DesktopConnection, closePage: () => void): BoardService {
    return createBoardService(desktop, {
        async load(account) {
            return (await ipcRenderer.invoke(
                "codex-enhancer:board-records",
                account,
            )) as LoadedBoardRecords;
        },
        async loadTaskCache(account) {
            return (await ipcRenderer.invoke("codex-enhancer:task-cache", account)) as Card[];
        },
        async saveTaskCache(account, cards) {
            await ipcRenderer.invoke("codex-enhancer:task-cache", account, cards);
        },
        async save(update) {
            await ipcRenderer.invoke("codex-enhancer:board-records", update);
        },
        async preparePhase(update) {
            return (await ipcRenderer.invoke("codex-enhancer:board-phase-intent", update)) === true;
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
        async titleEpoch(account) {
            return (await ipcRenderer.invoke(
                "codex-enhancer:board-title-epoch",
                account,
            )) as number;
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
        const { id, path, body, account } = value as BoardRelayRequest;
        void board.request(path, body, account).then(
            (result) => ipcRenderer.send("codex-enhancer:board-result", { id, value: result }),
            (error: unknown) =>
                ipcRenderer.send("codex-enhancer:board-result", {
                    id,
                    error: error instanceof Error ? error.message : String(error),
                }),
        );
    });
}

function desktopBoard(mountBoard: MountBoard, mountTasks?: MountTasks): void {
    const desktop = createDesktopConnection({
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
        ...(mountTasks
            ? { mountTasks: (container: HTMLElement) => mountTasks(container, board) }
            : {}),
        mountPage(id, panel, close, showConversation) {
            if (id !== "session-board") return;
            closePage = close;
            const unmount = mountBoard(
                panel,
                (path, body) =>
                    path === "/api/new"
                        ? board.createBesideBoard(body, () => showConversation(true, "draft"))
                        : board.request(path, body),
                close,
                {
                    async open(id) {
                        await board.openBesideBoard(id);
                    },
                    setVisible: showConversation,
                },
            );
            return () => {
                closePage = undefined;
                unmount();
            };
        },
    });
}

export function start(mountBoard: MountBoard, mountTasks?: MountTasks): void {
    proof("preload-entered", { protocol: location.protocol, top: window.top === window });
    if (
        location.origin !== "app://-" ||
        location.pathname !== "/index.html" ||
        window.top !== window
    )
        return;
    try {
        desktopBoard(mountBoard, mountTasks);
    } catch (error) {
        proof("preload-failed", {
            error: error instanceof Error ? (error.stack ?? error.message) : String(error),
        });
    }
}
