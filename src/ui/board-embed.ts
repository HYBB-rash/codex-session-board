import type { BoardRequest } from "./board-start";
import { mountBoardPage } from "./views/board/mount";

export function mountEnhancerBoard(panel: HTMLElement, request: BoardRequest, close: () => void) {
    const container = document.createElement("div");
    container.style.height = "100%";
    container.style.setProperty("--board-height", "100%");
    const root = container.attachShadow({ mode: "open" });
    panel.append(container);
    const unmount = mountBoardPage(root, request, { close });
    return () => {
        unmount();
        container.remove();
    };
}
