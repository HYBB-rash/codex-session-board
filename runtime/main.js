"use strict";

const fs = require("node:fs");
const path = require("node:path");

const runtimeDirectory = __dirname;
const home = path.resolve(runtimeDirectory, "..");
const logPath = path.join(home, "logs", "runtime.jsonl");
const shotDirectory = path.join(home, "logs", "screenshots");

function log(event, detail = {}) {
  fs.mkdirSync(path.dirname(logPath), { recursive: true });
  fs.appendFileSync(
    logPath,
    `${JSON.stringify({ at: new Date().toISOString(), event, ...detail })}\n`,
  );
}

function readAppInfo() {
  const resources = process.resourcesPath;
  const metadata = JSON.parse(fs.readFileSync(path.join(resources, "linux-package-metadata.json"), "utf8"));
  const owl = JSON.parse(fs.readFileSync(path.join(resources, "owl-electron-app.json"), "utf8"));
  const chromium = fs.readFileSync(path.join(resources, "..", "version"), "utf8").trim();
  return {
    codexVersion: metadata.version,
    runtimeType: owl.runtimeName || (process.versions.electron ? "electron" : "unknown"),
    chromiumVersion: chromium,
    electronVersion: process.versions.electron || null,
  };
}

function loadExtensions() {
  const directory = path.join(home, "extensions");
  const loaded = [];
  if (!fs.existsSync(directory)) return loaded;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const file = path.join(directory, entry.name, "extension.js");
    if (!fs.existsSync(file)) continue;
    const extension = require(file);
    const contribution = {};
    if (typeof extension.setup === "function") {
      extension.setup({
        log,
        contribute(value) {
          Object.assign(contribution, value);
        },
      });
    }
    loaded.push({ id: extension.id || entry.name, contribution });
  }
  return loaded;
}

async function capture(contents, fileName) {
  const image = await contents.capturePage();
  fs.mkdirSync(shotDirectory, { recursive: true });
  fs.writeFileSync(path.join(shotDirectory, fileName), image.toPNG());
  log("screenshot", { fileName, url: contents.getURL() });
}

function start() {
  log("loader-entered", { pid: process.pid, execPath: process.execPath });
  const { app, session, ipcMain, BrowserWindow } = require("electron");
  const info = readAppInfo();
  const extensions = loadExtensions();
  const buttonLabel = extensions.find((item) => item.contribution.buttonLabel)?.contribution.buttonLabel || "Enhancer";
  log("main-loaded", { ...info, extensions: extensions.map((item) => item.id), buttonLabel });

  const registered = new WeakSet();
  const preloadPath = path.join(runtimeDirectory, "preload.js");

  function installPreload(ses, reason) {
    if (!ses || registered.has(ses)) return;
    const register = () => {
      if (typeof ses.registerPreloadScript === "function") {
        ses.registerPreloadScript({ type: "frame", filePath: preloadPath, id: "codex-enhancer" });
        log("preload-registered", { reason, via: "registerPreloadScript" });
        return;
      }
      const current = typeof ses.getPreloads === "function" ? ses.getPreloads() : [];
      if (!current.includes(preloadPath) && typeof ses.setPreloads === "function") {
        ses.setPreloads([preloadPath, ...current]);
        log("preload-registered", { reason, via: "setPreloads" });
      }
    };
    try {
      register();
      registered.add(ses);
    } catch (error) {
      log("preload-register-failed", { reason, error: error instanceof Error ? error.stack : String(error) });
    }
  }

  app.on("session-created", (ses) => installPreload(ses, "session-created"));
  app.on("web-contents-created", (_event, contents) => {
    contents.on("did-finish-load", () => {
      log("did-finish-load", { id: contents.id, url: contents.getURL() });
    });
  });

  let opened = false;
  let pageWindow = null;

  function pagePayload() {
    return {
      ...info,
      buttonLabel,
      extensions: extensions.map((item) => item.id),
    };
  }

  ipcMain.handle("codex-enhancer:info", () => pagePayload());
  ipcMain.on("codex-enhancer:proof", (event, payload) => {
    log("renderer-proof", payload);
    if (payload?.stage === "button-mounted") {
      const contents = event.sender;
      const bounds = contents.getOwnerBrowserWindow?.()?.getBounds?.() || BrowserWindow.fromWebContents(contents)?.getBounds();
      if (bounds && bounds.width >= 480) {
        capture(contents, `official-${contents.id}.png`).catch((error) => {
          log("screenshot-failed", { error: String(error) });
        });
      }
      if (process.env.CODEX_ENHANCER_AUTO_OPEN === "1" && !opened) {
        opened = true;
        contents.send("codex-enhancer:auto-open");
      }
    }
    if (payload?.stage === "page-rendered") {
      capture(event.sender, "custom-page.png").catch((error) => {
        log("screenshot-failed", { error: String(error) });
      });
    }
  });
  ipcMain.on("codex-enhancer:close-page", (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close();
  });
  ipcMain.handle("codex-enhancer:open-page", async () => {
    if (pageWindow && !pageWindow.isDestroyed()) {
      pageWindow.focus();
      return { ok: true, reused: true };
    }
    pageWindow = new BrowserWindow({
      width: 560,
      height: 360,
      show: true,
      title: "Codex Enhancer",
      autoHideMenuBar: true,
      webPreferences: {
        preload: path.join(runtimeDirectory, "renderer.js"),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
      },
    });
    pageWindow.on("closed", () => {
      pageWindow = null;
    });
    await pageWindow.loadFile(path.join(runtimeDirectory, "page.html"));
    log("page-opened", pagePayload());
    return { ok: true };
  });

  const installExisting = () => installPreload(session.defaultSession, "default-session");
  const captureSettled = () => {
    setTimeout(() => {
      for (const win of BrowserWindow.getAllWindows()) {
        const contents = win.webContents;
        if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) continue;
        capture(contents, "official-settled.png").catch((error) => {
          log("screenshot-failed", { error: String(error) });
        });
      }
    }, 8000);
  };
  if (app.isReady()) {
    installExisting();
    captureSettled();
  } else {
    app.whenReady().then(() => {
      installExisting();
      captureSettled();
    });
  }
}

try {
  start();
} catch (error) {
  log("fatal", { error: error instanceof Error ? error.stack : String(error) });
}
