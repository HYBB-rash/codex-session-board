"use strict";

const { ipcRenderer } = require("electron");

window.addEventListener("DOMContentLoaded", async () => {
  const info = await ipcRenderer.invoke("codex-enhancer:info");
  document.getElementById("version").textContent = `Codex Version: ${info.codexVersion}`;
  document.getElementById("runtime").textContent = `Runtime Type: ${info.runtimeType}`;
  document.getElementById("close").addEventListener("click", () => {
    ipcRenderer.send("codex-enhancer:close-page");
  });
  ipcRenderer.send("codex-enhancer:proof", {
    stage: "page-rendered",
    codexVersion: info.codexVersion,
    runtimeType: info.runtimeType,
  });
});
