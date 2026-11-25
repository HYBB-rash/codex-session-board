if (
    process.platform !== "linux" ||
    process.execPath !== "/opt/codex-desktop/ChatGPT" ||
    !process.env.CODEX_ENHANCER_SMOKE_HOME ||
    process.env.HOME !== process.env.CODEX_ENHANCER_SMOKE_HOME
) {
    throw new Error("会话夹具只能装载到隔离的 Codex RPM 窗口");
}

const { app, ipcMain } = require("electron");
const boardContextChannel = "codex-enhancer:board-desktop";
app.on("web-contents-created", (_event, contents) => {
    contents.on("did-finish-load", () => {
        if (contents.getURL() !== "app://-/index.html") return;
        ipcMain.removeHandler(boardContextChannel);
        ipcMain.handle(boardContextChannel, (event, command) => {
            if (event.sender !== contents || command?.kind !== "context")
                throw new Error("会话夹具仅提供测试账号上下文");
            return { accountId: "rpm-smoke", userId: "rpm-smoke", sections: [] };
        });
        contents.send("codex_desktop:message-for-view", {
            type: "codex-app-server-connection-changed",
            hostId: "local",
            state: "connected",
        });
        void contents
            .executeJavaScript(`(() => {
            if (document.querySelector("[data-app-shell-workspace]")) return;
            const workspace = document.createElement("div");
            workspace.hidden = true;
            workspace.setAttribute("data-app-shell-workspace", "");
            document.body.append(workspace);
        })()`)
            .catch((error) => console.error("会话夹具准备失败", error));
    });
});
