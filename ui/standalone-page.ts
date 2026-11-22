import shell from "./board-shell.html?raw";
import { type BoardRequest, mountBoardPage } from "./board-start";

export function startBoard(transport: BoardRequest): void {
    const template = document.createElement("template");
    template.innerHTML = shell;
    document.body.append(template.content);
    const page = mountBoardPage(document, transport);
    window.addEventListener("pagehide", page.unmount, { once: true });
}
