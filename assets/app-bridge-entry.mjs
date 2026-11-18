import { App, applyDocumentTheme } from "@modelcontextprotocol/ext-apps";

const app = new App(
  { name: "Codex session board", version: "0.2.0" },
  { availableDisplayModes: ["inline", "fullscreen"] },
  { autoResize: false }
);
const applyContext = context => {
  if (context?.theme) applyDocumentTheme(context.theme);
  if (context?.safeAreaInsets) {
    for (const side of ["top", "right", "bottom", "left"]) {
      const inset = context.safeAreaInsets[side];
      if (Number.isFinite(inset) && inset >= 0)
        document.documentElement.style.setProperty(`--host-inset-${side}`, `${inset}px`);
    }
  }
};
app.onhostcontextchanged = applyContext;
const connected = app.connect().then(() => applyContext(app.getHostContext()));
window.boardOpenLink = async url => {
  await connected;
  const { isError } = await app.openLink({ url });
  if (isError) throw new Error("无法打开链接");
};
const routes = {
  "/api/sessions": "board_snapshot",
  "/api/state": "board_update",
  "/api/open": "board_open",
  "/api/preview-reply": "board_preview_reply",
  "/api/refresh": "board_refresh",
  "/api/new": "board_new",
  "/api/new/reset": "board_new_reset",
  "/api/project/assign": "board_project_assign",
  "/api/connect": "board_connect",
  "/api/ssh-setup": "board_ssh_setup",
  "/api/archive": "board_archive",
  "/api/archive-batches": "board_archive_batches",
  "/api/archive-batches/finish": "board_archive_batch_finish",
  "/api/archive-batches/check": "board_archive_batch_check",
  "/api/column-sort": "board_sort",
  "/api/plugin-update": "board_plugin_update_status",
};
window.boardRequest = async (route, body) => {
  await connected;
  const name = route === "/api/archive-batches" && body ? "board_archive_batch_start"
    : route === "/api/plugin-update" && body ? "board_plugin_update_start"
    : Object.hasOwn(routes, route) && routes[route];
  if (!name) throw new Error("未知看板操作");
  const result = await app.callServerTool({ name, arguments: body || {} });
  if (result.isError)
    throw new Error(result.content?.find(item => item.type === "text")?.text || "操作失败");
  if (name === "board_preview_reply" && result._meta?.previewReply)
    return result._meta.previewReply;
  if (result.structuredContent) return result.structuredContent;
  const content = result.content?.find(item => item.type === "text");
  if (!content) throw new Error("看板返回了空结果");
  return JSON.parse(content.text);
};
