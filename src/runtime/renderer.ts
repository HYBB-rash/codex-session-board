/// <reference lib="dom" />

import { ipcRenderer } from "electron";

function pageElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) throw new Error(`page element #${id} is missing`);
  return element;
}

function pageInfo(value: unknown): { codexVersion: string; runtimeType: string } {
  if (typeof value !== "object" || value === null) throw new Error("page info is incomplete");
  if (!("codexVersion" in value) || typeof value.codexVersion !== "string") {
    throw new Error("page info is missing the Codex version");
  }
  if (!("runtimeType" in value) || typeof value.runtimeType !== "string") {
    throw new Error("page info is missing the runtime type");
  }
  return { codexVersion: value.codexVersion, runtimeType: value.runtimeType };
}

window.addEventListener("DOMContentLoaded", async () => {
  const info = pageInfo(await ipcRenderer.invoke("codex-enhancer:info"));
  pageElement("version").textContent = `Codex Version: ${info.codexVersion}`;
  pageElement("runtime").textContent = `Runtime Type: ${info.runtimeType}`;
  pageElement("close").addEventListener("click", () => {
    ipcRenderer.send("codex-enhancer:close-page");
  });
  ipcRenderer.send("codex-enhancer:proof", {
    stage: "page-rendered",
    codexVersion: info.codexVersion,
    runtimeType: info.runtimeType,
  });
});
