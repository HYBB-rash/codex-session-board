import type { ExtensionApi } from "../runtime/api.js";

function setup(api: ExtensionApi): void {
    api.ui.registerSidebarItem({
        id: "session-board",
        title: "会话看板",
        icon: "layout-dashboard",
        route: "/session-board",
        page: {
            title: "会话看板",
            body: "当前窗口无法挂载会话看板。",
        },
    });
    api.log("session-board-loaded", { id: "session-board" });
}

export = {
    id: "session-board",
    setup,
};
