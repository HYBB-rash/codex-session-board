import { App, applyDocumentTheme, type McpUiHostContext } from "@modelcontextprotocol/ext-apps";
import operations from "../../shared/board/operations.json";
import { startBoard } from "./page";

const app = new App(
    { name: "Codex session board", version: __VERSION__ },
    { availableDisplayModes: ["inline", "fullscreen"] },
    { autoResize: false },
);

function applyContext(context: McpUiHostContext | undefined): void {
    if (context?.theme) applyDocumentTheme(context.theme);
    if (!context?.safeAreaInsets) return;
    for (const side of ["top", "right", "bottom", "left"] as const) {
        const inset = context.safeAreaInsets[side];
        if (Number.isFinite(inset) && inset >= 0) {
            document.documentElement.style.setProperty(`--host-inset-${side}`, `${inset}px`);
        }
    }
}

app.onhostcontextchanged = applyContext;
const connection = app.connect().then(() => applyContext(app.getHostContext()));
window.boardOpenLink = async (url) => {
    const { protocol } = new URL(url);
    if (protocol !== "https:" && protocol !== "http:") throw new Error("链接协议不受支持");
    await connection;
    const { isError } = await app.openLink({ url });
    if (isError) throw new Error("无法打开链接");
};

startBoard(async (path, body) => {
    await connection;
    const operation = operations.find(
        (item) => item.path === path && item.method === (body === undefined ? "GET" : "POST"),
    );
    if (!operation) throw new Error("未知看板操作");
    const result = await app.callServerTool({ name: operation.tool, arguments: { ...body } });
    const text = result.content?.find((item) => item.type === "text")?.text;
    if (result.isError) throw new Error(text || "看板操作失败");
    const { previewReply } = result._meta ?? {};
    if (operation.action === "PreviewReply" && previewReply) return previewReply;
    if (result.structuredContent) return result.structuredContent;
    if (text) return JSON.parse(text);
    throw new Error("看板返回了空结果");
});

declare const __VERSION__: string;
