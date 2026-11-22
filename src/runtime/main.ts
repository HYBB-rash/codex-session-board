import { createRequire } from "node:module";
import * as fs from "node:fs";
import * as path from "node:path";
import type { ExtensionApi, ExtensionContribution, ExtensionModule } from "./api.js";
import type { App, BrowserWindow, IpcMain, Session, WebContents } from "electron";

const requireFromHere = createRequire(__filename);
const runtimeDirectory = __dirname;
const home = path.resolve(runtimeDirectory, "..");
const logPath = path.join(home, "logs", "runtime.jsonl");
const shotDirectory = path.join(home, "logs", "screenshots");

interface RuntimeInfo {
  codexVersion: string;
  runtimeType: string;
  chromiumVersion: string;
  electronVersion: string | null;
}

interface ElectronModule {
  app: App;
  session: { defaultSession: Session };
  ipcMain: IpcMain;
  BrowserWindow: typeof import("electron").BrowserWindow;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function log(event: string, detail: object = {}): void {
  fs.mkdirSync(path.dirname(logPath), { recursive: true });
  fs.appendFileSync(
    logPath,
    `${JSON.stringify({ at: new Date().toISOString(), event, ...detail })}\n`,
  );
}

function readJson(filePath: string): unknown {
  return JSON.parse(fs.readFileSync(filePath, "utf8")) as unknown;
}

function requiredString(value: unknown, key: string, filePath: string): string {
  if (!isRecord(value) || typeof value[key] !== "string") {
    throw new Error(`${key} is missing in ${filePath}`);
  }
  return value[key];
}

function optionalString(value: unknown, key: string): string | null {
  if (!isRecord(value)) return null;
  const item = value[key];
  return typeof item === "string" ? item : null;
}

function readAppInfo(): RuntimeInfo {
  const resources = process.resourcesPath;
  const metadataPath = path.join(resources, "linux-package-metadata.json");
  const owlPath = path.join(resources, "owl-electron-app.json");
  const metadata = readJson(metadataPath);
  const owl = readJson(owlPath);
  const chromium = fs.readFileSync(path.join(resources, "..", "version"), "utf8").trim();
  const electronVersion = process.versions.electron ?? null;
  return {
    codexVersion: requiredString(metadata, "version", metadataPath),
    runtimeType: optionalString(owl, "runtimeName") || (electronVersion ? "electron" : "unknown"),
    chromiumVersion: chromium,
    electronVersion,
  };
}

function loadExtension(file: string): ExtensionModule {
  const loaded: unknown = requireFromHere(file);
  if (!isRecord(loaded)) throw new Error(`extension is not an object: ${file}`);
  const id = typeof loaded.id === "string" ? loaded.id : undefined;
  const setup = typeof loaded.setup === "function" ? loaded.setup as ExtensionModule["setup"] : undefined;
  return { id, setup };
}

function loadExtensions(): Array<{ id: string; contribution: ExtensionContribution }> {
  const directory = path.join(home, "extensions");
  const loaded: Array<{ id: string; contribution: ExtensionContribution }> = [];
  if (!fs.existsSync(directory)) return loaded;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const file = path.join(directory, entry.name, "extension.js");
    if (!fs.existsSync(file)) continue;
    const extension = loadExtension(file);
    const contribution: ExtensionContribution = {};
    if (typeof extension.setup === "function") {
      const api: ExtensionApi = {
        log,
        contribute(value) {
          Object.assign(contribution, value);
        },
      };
      extension.setup(api);
    }
    loaded.push({ id: extension.id || entry.name, contribution });
  }
  return loaded;
}

async function capture(contents: WebContents, fileName: string): Promise<void> {
  const image = await contents.capturePage();
  fs.mkdirSync(shotDirectory, { recursive: true });
  fs.writeFileSync(path.join(shotDirectory, fileName), image.toPNG());
  log("screenshot", { fileName, url: contents.getURL() });
}

function loadElectron(): ElectronModule {
  const loaded: unknown = requireFromHere("electron");
  if (!isRecord(loaded) || !("app" in loaded) || !("BrowserWindow" in loaded) || !("ipcMain" in loaded) || !("session" in loaded)) {
    throw new Error("electron did not export app, session, ipcMain, and BrowserWindow");
  }
  return loaded as unknown as ElectronModule;
}

function start(): void {
  log("loader-entered", { pid: process.pid, execPath: process.execPath });
  const { app, session, ipcMain, BrowserWindow } = loadElectron();
  const info = readAppInfo();
  const extensions = loadExtensions();
  const buttonLabel = extensions.find((item) => item.contribution.buttonLabel)?.contribution.buttonLabel || "Enhancer";
  log("main-loaded", { ...info, extensions: extensions.map((item) => item.id), buttonLabel });

  const registered = new WeakSet<Session>();
  const preloadPath = path.join(runtimeDirectory, "preload.js");

  function installPreload(ses: Session | null | undefined, reason: string): void {
    if (!ses || registered.has(ses)) return;
    const register = (): void => {
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
  let pageWindow: BrowserWindow | null = null;

  function pagePayload(): RuntimeInfo & { buttonLabel: string; extensions: string[] } {
    return {
      ...info,
      buttonLabel,
      extensions: extensions.map((item) => item.id),
    };
  }

  ipcMain.handle("codex-enhancer:info", () => pagePayload());
  ipcMain.on("codex-enhancer:proof", (event, payload: unknown) => {
    const detail = isRecord(payload) ? payload : { payload: String(payload) };
    log("renderer-proof", detail);
    if (detail.stage === "button-mounted") {
      const contents = event.sender;
      const bounds = contents.getOwnerBrowserWindow?.()?.getBounds() || BrowserWindow.fromWebContents(contents)?.getBounds();
      if (bounds && bounds.width >= 480) {
        capture(contents, `official-${contents.id}.png`).catch((error: unknown) => {
          log("screenshot-failed", { error: String(error) });
        });
      }
      if (process.env.CODEX_ENHANCER_AUTO_OPEN === "1" && !opened) {
        opened = true;
        contents.send("codex-enhancer:auto-open");
      }
    }
    if (detail.stage === "page-rendered") {
      capture(event.sender, "custom-page.png").catch((error: unknown) => {
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

  const installExisting = (): void => installPreload(session.defaultSession, "default-session");
  const captureSettled = (): void => {
    setTimeout(() => {
      for (const win of BrowserWindow.getAllWindows()) {
        const contents = win.webContents;
        if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) continue;
        capture(contents, "official-settled.png").catch((error: unknown) => {
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
