"use strict";

const { ipcRenderer } = require("electron");

const buttonId = "codex-enhancer-button";

function isOfficialUi() {
  return location.protocol === "app:";
}

function isEnhancerPage() {
  return document.documentElement?.dataset?.codexEnhancerPage === "1";
}

function proof(stage, extra = {}) {
  ipcRenderer.send("codex-enhancer:proof", { stage, href: location.href, ...extra });
}

function mount(label) {
  if (!isOfficialUi() || isEnhancerPage() || !document.body || document.getElementById(buttonId)) return;
  const button = document.createElement("button");
  button.id = buttonId;
  button.type = "button";
  button.textContent = label;
  button.style.cssText = [
    "position:fixed",
    "top:12px",
    "right:12px",
    "z-index:2147483647",
    "padding:8px 12px",
    "border:0",
    "border-radius:8px",
    "background:#f5c400",
    "color:#111",
    "font:700 14px sans-serif",
    "cursor:pointer",
    "box-shadow:0 2px 8px rgba(0,0,0,.35)",
  ].join(";");
  button.addEventListener("click", () => {
    proof("button-clicked");
    ipcRenderer.invoke("codex-enhancer:open-page").catch((error) => {
      proof("open-failed", { error: String(error) });
    });
  });
  document.body.appendChild(button);
  proof("button-mounted");
}

function watch(label) {
  const refresh = () => mount(label);
  refresh();
  const observer = new MutationObserver(refresh);
  observer.observe(document.documentElement, { childList: true, subtree: true });
  window.addEventListener("load", refresh, { once: true });
}

ipcRenderer.on("codex-enhancer:auto-open", () => {
  document.getElementById(buttonId)?.click();
});

ipcRenderer.invoke("codex-enhancer:info").then(
  (info) => {
    const label = info?.buttonLabel || "Enhancer";
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", () => watch(label), { once: true });
    } else {
      watch(label);
    }
  },
  (error) => proof("info-failed", { error: String(error) }),
);
