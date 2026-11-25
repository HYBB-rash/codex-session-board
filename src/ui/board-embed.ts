import { mountScopedBoard } from "./board-page";
import shell from "./board-shell.html?raw";
import type { BoardRequest } from "./board-start";

export function mountEnhancerBoard(panel: HTMLElement, request: BoardRequest, close: () => void) {
    const container = document.createElement("div");
    container.style.height = "100%";
    const root = container.attachShadow({ mode: "open" });
    panel.append(container);
    return mountScopedBoard(root, request, () => {
        const template = document.createElement("template");
        template.innerHTML = shell;
        const style = template.content.querySelector("style");
        if (style?.textContent) {
            style.textContent = style.textContent
                .replace(/:root\b/g, ":host")
                .replace(/html\s*,\s*body\s*\{\s*height:\s*100%\s*;?\s*\}/, "")
                .replace(/(^|\n)body\s*\{/g, "$1:host { display: block; height: 100%;")
                .replace(/(\.shell\s*\{\s*height:\s*)100dvh/, (_, rule: string) => `${rule}100%`);
        }
        const back = document.createElement("button");
        back.type = "button";
        back.textContent = "关闭看板";
        back.className = "quiet";
        back.onclick = close;
        template.content.querySelector(".header-actions")?.prepend(back);
        return template.content;
    });
}
