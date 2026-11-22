import * as fs from "node:fs";
import { createRequire } from "node:module";
import * as path from "node:path";
import type { App, IpcMain, IpcMainEvent, Session, WebContents } from "electron";
import { logs, outputDir } from "../installer/paths.js";
import type { ExtensionApi, ExtensionModule, SidebarItemInput } from "./api.js";
import { trustedAppFrame } from "./ipc-guard.js";
import { type OwnedSidebarItem, SidebarRegistry } from "./registry.js";

const requireFromHere = createRequire(__filename);
const runtimeDirectory = __dirname;
const logPath = path.join(logs, "runtime.jsonl");
const shotDirectory = path.join(logs, "screenshots");

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
        runtimeType:
            optionalString(owl, "runtimeName") || (electronVersion ? "electron" : "unknown"),
        chromiumVersion: chromium,
        electronVersion,
    };
}

function loadExtension(file: string): ExtensionModule {
    const loaded: unknown = requireFromHere(file);
    if (!isRecord(loaded)) throw new Error(`extension is not an object: ${file}`);
    const id = typeof loaded.id === "string" ? loaded.id : undefined;
    const setup =
        typeof loaded.setup === "function" ? (loaded.setup as ExtensionModule["setup"]) : undefined;
    return { id, setup };
}

function loadExtensions(registry: SidebarRegistry, publish: () => void): string[] {
    const directory = path.join(outputDir, "extensions");
    const ids: string[] = [];
    if (!fs.existsSync(directory)) return ids;
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const file = path.join(directory, entry.name, "extension.js");
        if (!fs.existsSync(file)) continue;
        const extension = loadExtension(file);
        const extensionId = extension.id || entry.name;
        if (typeof extension.setup === "function") {
            const api: ExtensionApi = {
                log,
                ui: {
                    registerSidebarItem(item: SidebarItemInput) {
                        const remove = registry.register(extensionId, item);
                        publish();
                        return () => {
                            remove();
                            publish();
                        };
                    },
                },
            };
            extension.setup(api);
        }
        ids.push(extensionId);
    }
    return ids;
}

async function capture(contents: WebContents, fileName: string): Promise<void> {
    const image = await contents.capturePage();
    fs.mkdirSync(shotDirectory, { recursive: true });
    fs.writeFileSync(path.join(shotDirectory, fileName), image.toPNG());
    log("screenshot", { fileName, url: contents.getURL() });
}

function loadElectron(): ElectronModule {
    const loaded: unknown = requireFromHere("electron");
    if (
        !isRecord(loaded) ||
        !("app" in loaded) ||
        !("BrowserWindow" in loaded) ||
        !("ipcMain" in loaded) ||
        !("session" in loaded)
    ) {
        throw new Error("electron did not export app, session, ipcMain, and BrowserWindow");
    }
    return loaded as unknown as ElectronModule;
}

function uiPayload(registry: SidebarRegistry): { items: OwnedSidebarItem[]; autoOpen: boolean } {
    return {
        items: registry.list(),
        autoOpen: process.env.CODEX_ENHANCER_AUTO_OPEN === "1",
    };
}

function start(): void {
    log("loader-entered", { pid: process.pid, execPath: process.execPath });
    const { app, session, ipcMain, BrowserWindow } = loadElectron();
    const info = readAppInfo();
    const registry = new SidebarRegistry();
    const appContents = new Set<WebContents>();

    function publish(): void {
        const payload = uiPayload(registry);
        for (const contents of appContents) {
            if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) {
                appContents.delete(contents);
                continue;
            }
            contents.send("codex-enhancer:ui-items", payload);
        }
    }

    const extensions = loadExtensions(registry, publish);
    log("main-loaded", { ...info, extensions, items: registry.list().map((item) => item.id) });

    const registered = new WeakSet<Session>();
    const preloadPath = path.join(runtimeDirectory, "preload.js");

    function installPreload(ses: Session | null | undefined, reason: string): void {
        if (!ses || registered.has(ses)) return;
        const register = (): void => {
            if (typeof ses.registerPreloadScript === "function") {
                ses.registerPreloadScript({
                    type: "frame",
                    filePath: preloadPath,
                    id: "codex-enhancer",
                });
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
            log("preload-register-failed", {
                reason,
                error: error instanceof Error ? error.stack : String(error),
            });
        }
    }

    function remember(contents: WebContents): void {
        if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) return;
        appContents.add(contents);
        publish();
    }

    function trusted(event: IpcMainEvent): boolean {
        const frame = event.senderFrame;
        const accepted = trustedAppFrame(frame);
        if (!accepted) {
            log("ipc-rejected", {
                url: frame?.url ?? null,
                parent:
                    frame == null
                        ? "missing-frame"
                        : frame.parent === null
                          ? "null"
                          : typeof frame.parent,
            });
        }
        return accepted;
    }

    app.on("session-created", (ses) => installPreload(ses, "session-created"));
    app.on("web-contents-created", (_event, contents) => {
        contents.on("did-finish-load", () => {
            log("did-finish-load", { id: contents.id, url: contents.getURL() });
            remember(contents);
        });
        contents.on("did-navigate", () => remember(contents));
        contents.on("destroyed", () => appContents.delete(contents));
    });

    ipcMain.handle("codex-enhancer:ui-items", (event) => {
        if (!trusted(event)) throw new Error("enhancer IPC rejected");
        return uiPayload(registry);
    });
    ipcMain.on("codex-enhancer:proof", (event, payload: unknown) => {
        if (!trusted(event)) return;
        const detail = isRecord(payload) ? payload : { payload: String(payload) };
        log("renderer-proof", detail);
        const stage = detail.stage;
        const contents = event.sender;
        const bounds =
            contents.getOwnerBrowserWindow?.()?.getBounds() ||
            BrowserWindow.fromWebContents(contents)?.getBounds();
        if (bounds == null || bounds.width < 480) return;
        const fileName =
            stage === "sidebar-mounted"
                ? "sidebar-mounted.png"
                : stage === "page-rendered"
                  ? "session-board.png"
                  : stage === "shell-observed"
                    ? "shell-observed.png"
                    : stage === "page-closed"
                      ? "page-closed.png"
                      : null;
        if (fileName == null) return;
        capture(contents, fileName).catch((error: unknown) => {
            log("screenshot-failed", { error: String(error) });
        });
    });

    const installExisting = (): void => installPreload(session.defaultSession, "default-session");
    if (app.isReady()) installExisting();
    else app.whenReady().then(installExisting);
}

try {
    start();
} catch (error) {
    log("fatal", { error: error instanceof Error ? error.stack : String(error) });
}
