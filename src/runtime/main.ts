import * as fs from "node:fs";
import { createRequire } from "node:module";
import * as path from "node:path";
import type { App, IpcMain, IpcMainEvent, Session, WebContents } from "electron";
import type { BoardLedger } from "../extensions/archive.js";
import { boardDesktopRequest } from "../extensions/desktop/main.js";
import {
    type BoardRecords,
    createBoardLedgers,
    createBoardRecords,
    type PhaseIntent,
    type RecordWrite,
    startTitleEpoch,
    type TitleResolution,
} from "../extensions/records.js";
import { inPlaceRuntime, logs, outputDir, stateDir } from "../installer/paths.js";
import { runningVersion } from "../installer/release/store.js";
import { LegacyBoardRuntime } from "../migration/runtime.js";
import type { ExtensionApi, ExtensionModule, SidebarItemInput } from "./api.js";
import { BoardBroker, boardFrame } from "./board/broker.js";
import { trustedAppFrame } from "./ipc/guard.js";
import { type OwnedSidebarItem, SidebarRegistry } from "./registry.js";

const requireFromHere = createRequire(__filename);
const runtimeDirectory = __dirname;
const preloadPath = path.join(runtimeDirectory, "preload.js");
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

interface PackageMetadata {
    version: unknown;
}

interface OwlApp {
    runtimeName?: unknown;
}

interface RawExtension {
    id?: unknown;
    setup?: unknown;
}

interface RendererProof {
    stage?: unknown;
}

function isFields<T extends object>(value: unknown): value is T {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return isFields<Record<string, unknown>>(value);
}

function log(event: string, detail: object = {}): void {
    fs.mkdirSync(path.dirname(logPath), { recursive: true, mode: 0o700 });
    fs.appendFileSync(
        logPath,
        `${JSON.stringify({ at: new Date().toISOString(), event, ...detail })}\n`,
        { mode: 0o600 },
    );
}

function readJson(filePath: string): unknown {
    return JSON.parse(fs.readFileSync(filePath, "utf8")) as unknown;
}

function requiredString(value: unknown, name: string, filePath: string): string {
    if (typeof value !== "string") throw new Error(`${name} is missing in ${filePath}`);
    return value;
}

function optionalString(value: unknown): string | null {
    return typeof value === "string" ? value : null;
}

function readAppInfo(app: App): RuntimeInfo {
    const resources = process.resourcesPath;
    const owlPath = path.join(resources, "owl-electron-app.json");
    const owlJson = fs.existsSync(owlPath) ? readJson(owlPath) : null;
    const owl = isFields<OwlApp>(owlJson) ? owlJson : null;
    const electronVersion = process.versions.electron ?? null;
    const runtimeType =
        optionalString(owl?.runtimeName) || (electronVersion ? "electron" : "unknown");
    if (process.platform === "darwin") {
        return {
            codexVersion: app.getVersion(),
            runtimeType,
            chromiumVersion: process.versions.chrome ?? "unknown",
            electronVersion,
        };
    }
    const metadataPath = path.join(resources, "linux-package-metadata.json");
    const metadataJson = readJson(metadataPath);
    const metadata = isFields<PackageMetadata>(metadataJson) ? metadataJson : null;
    const chromium = fs.readFileSync(path.join(resources, "..", "version"), "utf8").trim();
    return {
        codexVersion: requiredString(metadata?.version, "version", metadataPath),
        runtimeType,
        chromiumVersion: chromium,
        electronVersion,
    };
}

function loadExtension(file: string): ExtensionModule {
    const loaded: unknown = requireFromHere(file);
    if (!isFields<RawExtension>(loaded)) throw new Error(`extension is not an object: ${file}`);
    const extension: ExtensionModule = {};
    if (typeof loaded.id === "string") extension.id = loaded.id;
    if (typeof loaded.setup === "function")
        extension.setup = loaded.setup as (api: ExtensionApi) => void;
    return extension;
}

function loadExtensions(registry: SidebarRegistry, publish: () => void): string[] {
    const directory = path.join(outputDir, "extensions");
    const ids: string[] = [];
    if (!fs.existsSync(directory)) return ids;
    const directories = [
        directory,
        ...fs
            .readdirSync(directory, { withFileTypes: true })
            .filter((entry) => entry.isDirectory())
            .map((entry) => path.join(directory, entry.name)),
    ];
    for (const location of directories) {
        const file = path.join(location, "extension.js");
        if (!fs.existsSync(file)) continue;
        const extension = loadExtension(file);
        const extensionId = extension.id || path.basename(location);
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

function broadcast(registry: SidebarRegistry, appContents: Set<WebContents>): void {
    const payload = uiPayload(registry);
    for (const contents of appContents) {
        if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) {
            appContents.delete(contents);
            continue;
        }
        contents.send("codex-enhancer:ui-items", payload);
    }
}

function registerPreload(ses: Session, reason: string): void {
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
}

function installPreload(
    ses: Session | null | undefined,
    reason: string,
    registered: WeakSet<Session>,
): void {
    if (!ses || registered.has(ses)) return;
    try {
        registerPreload(ses, reason);
        registered.add(ses);
    } catch (error) {
        log("preload-register-failed", {
            reason,
            error: error instanceof Error ? error.stack : String(error),
        });
    }
}

function rememberContents(
    contents: WebContents,
    appContents: Set<WebContents>,
    publish: () => void,
): void {
    if (contents.isDestroyed() || !contents.getURL().startsWith("app://")) return;
    appContents.add(contents);
    publish();
}

function watchContents(app: App, appContents: Set<WebContents>, publish: () => void): void {
    app.on("web-contents-created", (_event, contents) => {
        contents.on("did-finish-load", () => {
            log("did-finish-load", { id: contents.id, url: contents.getURL() });
            rememberContents(contents, appContents, publish);
        });
        contents.on("did-navigate", () => rememberContents(contents, appContents, publish));
        contents.on("destroyed", () => appContents.delete(contents));
    });
}

function rejectionDetail(event: IpcMainEvent): Record<string, unknown> {
    const frame = event.senderFrame;
    const parent =
        frame == null ? "missing-frame" : frame.parent === null ? "null" : typeof frame.parent;
    return { url: frame?.url ?? null, parent };
}

function trustedSender(event: IpcMainEvent): boolean {
    if (trustedAppFrame(event.senderFrame)) return true;
    log("ipc-rejected", rejectionDetail(event));
    return false;
}

function screenshotName(stage: unknown): string | null {
    switch (stage) {
        case "sidebar-mounted":
            return "sidebar-mounted.png";
        case "page-rendered":
            return "session-board.png";
        case "shell-observed":
            return "shell-observed.png";
        case "page-closed":
            return "page-closed.png";
        default:
            return null;
    }
}

function captureProof(
    event: IpcMainEvent,
    payload: unknown,
    BrowserWindow: ElectronModule["BrowserWindow"],
): void {
    if (!trustedSender(event)) return;
    const detail = isFields<RendererProof>(payload) ? payload : { payload: String(payload) };
    log("renderer-proof", detail);
    if (inPlaceRuntime) return;
    const stage = isFields<RendererProof>(payload) ? payload.stage : undefined;
    const contents = event.sender;
    const bounds =
        contents.getOwnerBrowserWindow?.()?.getBounds() ||
        BrowserWindow.fromWebContents(contents)?.getBounds();
    if (bounds == null || bounds.width < 480) return;
    const fileName = screenshotName(stage);
    if (fileName == null) return;
    capture(contents, fileName).catch((error: unknown) => {
        log("screenshot-failed", { error: String(error) });
    });
}

function registerIpc(
    ipcMain: IpcMain,
    registry: SidebarRegistry,
    BrowserWindow: ElectronModule["BrowserWindow"],
): void {
    ipcMain.handle("codex-enhancer:ui-items", (event) => {
        if (!trustedSender(event)) throw new Error("enhancer IPC rejected");
        return uiPayload(registry);
    });
    ipcMain.handle("codex-enhancer:board-desktop", (event, value) => {
        if (!boardFrame(event)) throw new Error("enhancer IPC rejected");
        return boardDesktopRequest(event, value);
    });
    registerBoardStorage(ipcMain);
    ipcMain.on("codex-enhancer:proof", (event, payload: unknown) => {
        captureProof(event, payload, BrowserWindow);
    });
}

function registerBoardStorage(ipcMain: IpcMain): void {
    const directory = path.join(stateDir, "session-board");
    const boardRecords = createBoardRecords(path.join(directory, "records.json"));
    const ledgers = createBoardLedgers(path.join(directory, "ledger.json"));
    const titleEpochFile = path.join(directory, "title-activation.json");
    const legacy = new LegacyBoardRuntime(directory);
    ipcMain.handle("codex-enhancer:board-ledger", (event, account, value) => {
        if (!boardFrame(event) || typeof account !== "string")
            throw new Error("enhancer IPC rejected");
        const imported = legacy.forAccount(account)?.ledger;
        if (value === undefined) return ledgers.load(account, imported);
        if (
            !isFields<BoardLedger>(value) ||
            !Array.isArray(value.batches) ||
            !isRecord(value.columnSorts)
        ) {
            throw new Error("Invalid board ledger");
        }
        ledgers.save(account, value, imported);
        return null;
    });
    ipcMain.handle("codex-enhancer:board-config", (event, account) => {
        if (!boardFrame(event) || typeof account !== "string")
            throw new Error("enhancer IPC rejected");
        const config = path.join(directory, "config.json");
        return legacy.settings(account, fs.existsSync(config) ? readJson(config) : undefined);
    });
    ipcMain.handle("codex-enhancer:board-title-epoch", (event, account) => {
        if (!boardFrame(event) || typeof account !== "string")
            throw new Error("enhancer IPC rejected");
        return startTitleEpoch(titleEpochFile, account, legacy.titleEpoch(account));
    });
    registerBoardRecords(ipcMain, boardRecords, legacy);
}

function registerBoardRecords(
    ipcMain: IpcMain,
    boardRecords: BoardRecords,
    legacy: LegacyBoardRuntime,
): void {
    ipcMain.handle("codex-enhancer:board-phase-intent", (event, value) => {
        if (
            !boardFrame(event) ||
            !isFields<PhaseIntent>(value) ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            typeof value.phaseKey !== "string"
        )
            throw new Error("Invalid board phase intent");
        return boardRecords.preparePhase(value, legacy.forAccount(value.account)?.board.cards);
    });
    ipcMain.handle("codex-enhancer:board-records", (event, value) => {
        if (!boardFrame(event)) throw new Error("enhancer IPC rejected");
        if (typeof value === "string") return legacy.loadRecords(value, boardRecords);
        if (
            !isFields<RecordWrite>(value) ||
            typeof value.record?.note !== "string" ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            value.record.version !== value.expected + 1
        ) {
            throw new Error("Invalid board record");
        }
        boardRecords.write(value, legacy.forAccount(value.account)?.board.cards);
        return null;
    });
    ipcMain.handle("codex-enhancer:board-title-result", (event, value) => {
        if (
            !boardFrame(event) ||
            !isFields<TitleResolution>(value) ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            (value.failure !== undefined && typeof value.failure !== "string")
        ) {
            throw new Error("Invalid board title result");
        }
        return boardRecords.settleTitle(value, legacy.forAccount(value.account)?.board.cards);
    });
}

function start(): void {
    if (inPlaceRuntime) process.env.CODEX_ENHANCER_STATE_DIR = stateDir;
    log("loader-entered", { pid: process.pid, execPath: process.execPath });
    const { app, session, ipcMain, BrowserWindow } = loadElectron();
    const info = readAppInfo(app);
    const registry = new SidebarRegistry();
    const appContents = new Set<WebContents>();
    const publish = (): void => broadcast(registry, appContents);
    const extensions = loadExtensions(registry, publish);
    log("main-loaded", {
        ...info,
        boardVersion: runningVersion(),
        extensions,
        items: registry.list().map((item) => item.id),
    });
    const registered = new WeakSet<Session>();
    app.on("session-created", (ses) => installPreload(ses, "session-created", registered));
    watchContents(app, appContents, publish);
    registerIpc(ipcMain, registry, BrowserWindow);
    const board = new BoardBroker();
    board.register(ipcMain);
    void board
        .listen(log)
        .catch((error: unknown) => log("board-listen-failed", { error: String(error) }));
    const installExisting = (): void =>
        installPreload(session.defaultSession, "default-session", registered);
    if (app.isReady()) installExisting();
    else app.whenReady().then(installExisting);
}

try {
    start();
} catch (error) {
    log("fatal", { error: error instanceof Error ? error.stack : String(error) });
}
