import { h, render } from "vue";
import BoardPage from "./BoardPage.vue";
import type { BoardRequest, BoardSnapshot } from "./controller";
import type { BoardInstall } from "./install";
import styles from "./page.css?inline";

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
