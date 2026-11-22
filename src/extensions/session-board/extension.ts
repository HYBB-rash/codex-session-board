import type { ExtensionApi } from "../../runtime/api.js";

function setup(api: ExtensionApi): void {
    api.ui.registerSidebarItem({
        id: "session-board",
        title: "Session Board",
        icon: "layout-dashboard",
        route: "/session-board",
        page: {
            title: "Session Board",
            body: "Codex Enhancer UI integration successful.",
        },
    });
    api.log("session-board-loaded", { id: "session-board" });
}

export = {
    id: "session-board",
    setup,
};
