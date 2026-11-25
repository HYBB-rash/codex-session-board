import { mountBoardPage } from "./board-page";
import type { BoardInstall } from "./board-pwa";
import type { BoardRequest } from "./board-start";

export function startBoard(transport: BoardRequest, install?: BoardInstall): void {
    const unmount = mountBoardPage(document.body, transport, install ? { install } : {});
    window.addEventListener(
        "pagehide",
        () => {
            unmount();
            install?.dispose();
        },
        { once: true },
    );
}
