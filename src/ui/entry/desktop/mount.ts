import type { MountBoard } from "../../../runtime/preload";
import { mountBoardPage } from "../../mount";

export const mountEnhancerBoard: MountBoard = (panel, request, close, conversation) => {
    const container = document.createElement("div");
    container.style.height = "100%";
    container.style.setProperty("--board-height", "100%");
    const root = container.attachShadow({ mode: "open" });
    panel.append(container);
    const unmount = mountBoardPage(root, request, { close, conversation });
    return () => {
        unmount();
        container.remove();
    };
};
