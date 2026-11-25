import type { BoardRequest } from "../../views/board/controller";
import type { BoardInstall } from "../../views/board/install";
import { mountBoardPage } from "../../views/board/mount";

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
