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
const routes = {
  "/api/sessions": "board_snapshot",
  "/api/state": "board_update",
  "/api/open": "board_open",
  "/api/refresh": "board_refresh",
  "/api/new": "board_new",
  "/api/new/reset": "board_new_reset",
  "/api/project/assign": "board_project_assign",
  "/api/connect": "board_connect",
  "/api/archive": "board_archive",
  "/api/column-sort": "board_sort",
};
window.boardRequest = async (route, body) => {
  await connected;
  const name = Object.hasOwn(routes, route) && routes[route];
  if (!name) throw new Error("未知看板操作");
  const result = await app.callServerTool({ name, arguments: body || {} });
  if (result.isError)
    throw new Error(result.content?.find(item => item.type === "text")?.text || "操作失败");
  if (result.structuredContent) return result.structuredContent;
  const content = result.content?.find(item => item.type === "text");
  if (!content) throw new Error("看板返回了空结果");
  return JSON.parse(content.text);
};
