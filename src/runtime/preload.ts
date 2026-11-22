/// <reference lib="dom" />

import { ipcRenderer } from "electron";
import { startUiHost } from "./host.js";

function proof(stage: string, extra: Record<string, unknown> = {}): void {
  ipcRenderer.send("codex-enhancer:proof", { stage, href: location.href, ...extra });
}

proof("preload-entered", {
  protocol: location.protocol,
  top: window.top === window,
});

if (location.protocol === "app:" && window.top === window) {
  try {
    startUiHost({
      fetchState: () => ipcRenderer.invoke("codex-enhancer:ui-items"),
      subscribe(listener) {
        ipcRenderer.on("codex-enhancer:ui-items", (_event, payload) => listener(payload));
      },
      proof,
    });
  } catch (error) {
    proof("preload-failed", { error: error instanceof Error ? error.stack ?? error.message : String(error) });
  }
}
