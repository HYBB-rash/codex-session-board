import { h, render } from "vue";
import BoardPage from "./BoardPage.vue";
import styles from "./board-page.css?inline";
import type { BoardInstall } from "./board-pwa";
import type { BoardRequest, BoardSnapshot } from "./board-start";

export function mountBoardPage(
    root: HTMLElement | ShadowRoot,
    request: BoardRequest,
    options: { close?: () => void; install?: BoardInstall } = {},
) {
    const stylesElement = document.createElement("style");
    stylesElement.textContent = styles;
    const container = document.createElement("div");
    container.style.display = "contents";
    root.append(stylesElement, container);

    function mount(snapshot?: BoardSnapshot) {
        render(
            h(BoardPage, {
                ...options,
                key: snapshot?.accountScope ?? 0,
                request,
                replaceAccount: mount,
                ...(snapshot ? { snapshot } : {}),
            }),
            container,
        );
    }

    mount();
    return () => {
        render(null, container);
        container.remove();
        stylesElement.remove();
    };
}
