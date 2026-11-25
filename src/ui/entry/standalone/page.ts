import type { BoardRequest } from "../../controller";
import type { BoardInstall } from "../../install";
import { mountBoardPage } from "../../mount";

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
