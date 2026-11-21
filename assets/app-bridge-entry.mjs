import { App, applyDocumentTheme } from "@modelcontextprotocol/ext-apps";
import { routes, previewTool } from "./bridge-routes.json";

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
window.boardRequest = async (route, body) => {
  await connected;
  const methods = body === undefined ? routes.GET : routes.POST;
  if (!Object.hasOwn(methods, route)) throw new Error("未知看板操作");
  const name = methods[route];
  const result = await app.callServerTool({ name, arguments: body === undefined ? {} : body });
  if (result.isError)
    throw new Error(result.content?.find(item => item.type === "text")?.text || "操作失败");
  if (name === previewTool && result._meta?.previewReply)
    return result._meta.previewReply;
  if (result.structuredContent) return result.structuredContent;
  const content = result.content?.find(item => item.type === "text");
  if (!content) throw new Error("看板返回了空结果");
  return JSON.parse(content.text);
};
