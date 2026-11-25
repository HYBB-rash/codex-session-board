import type { BoardRequest } from "../../controller";
import { mountBoardPage } from "../../mount";
import type { BoardInstall } from "../../views/board/install";

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
