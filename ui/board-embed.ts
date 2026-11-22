import shell from "./board-shell.html?raw";
import { type BoardRequest, mountBoardPage } from "./board-start";

export function mountEnhancerBoard(panel: HTMLElement, request: BoardRequest, close: () => void) {
    const container = document.createElement("div");
    container.style.height = "100%";
    const root = container.attachShadow({ mode: "open" });
    const template = document.createElement("template");
    template.innerHTML = shell;
    const style = template.content.querySelector("style");
    if (style?.textContent) {
        style.textContent = style.textContent.replace(":root{", ":host{").replace("html,body{height:100%}", "")
            .replace("body{margin:0", ":host{display:block;height:100%;margin:0").replace(".shell{height:100dvh", ".shell{height:100%");
    }
    root.append(template.content);
    const back = document.createElement("button");
    back.type = "button";
    back.textContent = "关闭看板";
    back.className = "quiet";
    back.onclick = close;
    root.querySelector(".header-actions")?.prepend(back);
    panel.append(container);
    const page = mountBoardPage(root, request);
    return () => page.unmount();
}
