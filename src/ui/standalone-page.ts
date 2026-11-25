import shell from "./board-shell.html?raw";
import { mountScopedBoard } from "./board-page";
import type { BoardRequest } from "./board-start";

export function startBoard(transport: BoardRequest): void {
    const unmount = mountScopedBoard(document.body, transport, () => {
        const template = document.createElement("template");
        template.innerHTML = shell;
        return template.content;
    });
    window.addEventListener("pagehide", unmount, { once: true });
}
