declare namespace NodeJS {
    interface Process {
        resourcesPath: string;
    }
}

declare module "electron" {
    interface Rectangle {
        x: number;
        y: number;
        width: number;
        height: number;
    }

    interface NativeImage {
        toPNG(): Buffer;
    }

    interface WebFrameMain {
        url: string;
        parent: WebFrameMain | null;
    }

    interface WebContents {
        id: number;
        capturePage(): Promise<NativeImage>;
        getURL(): string;
        isDestroyed(): boolean;
        send(channel: string, ...args: unknown[]): void;
        on(event: "did-finish-load" | "did-navigate" | "destroyed", listener: () => void): this;
        getOwnerBrowserWindow?: () => BrowserWindow | null;
    }

    interface BrowserWindow {
        webContents: WebContents;
        isDestroyed(): boolean;
        focus(): void;
        close(): void;
        loadFile(filePath: string): Promise<void>;
        on(event: "closed", listener: () => void): this;
        getBounds(): Rectangle;
    }

    interface BrowserWindowOptions {
        width: number;
        height: number;
        show: boolean;
        title: string;
        autoHideMenuBar: boolean;
        webPreferences: {
            preload: string;
            contextIsolation: boolean;
            nodeIntegration: boolean;
            sandbox: boolean;
        };
    }

    interface Session {
        registerPreloadScript?: (script: { type: string; filePath: string; id: string }) => void;
        getPreloads?: () => string[];
        setPreloads?: (preloads: string[]) => void;
    }

    interface IpcMainEvent {
        sender: WebContents;
        senderFrame: WebFrameMain | null;
    }

    interface IpcMain {
        handle(
            channel: string,
            listener: (event: IpcMainEvent, ...args: unknown[]) => unknown,
        ): void;
        on(channel: string, listener: (event: IpcMainEvent, ...args: unknown[]) => void): void;
    }

    interface IpcRenderer {
        send(channel: string, ...args: unknown[]): void;
        invoke(channel: string, ...args: unknown[]): Promise<unknown>;
        on(channel: string, listener: (event: unknown, ...args: unknown[]) => void): void;
    }

    interface App {
        on(event: "session-created", listener: (ses: Session) => void): this;
        on(
            event: "web-contents-created",
            listener: (event: unknown, contents: WebContents) => void,
        ): this;
        isReady(): boolean;
        whenReady(): Promise<void>;
    }

    export const app: App;
    export const session: { defaultSession: Session };
    export const ipcMain: IpcMain;
    export const ipcRenderer: IpcRenderer;
    export const BrowserWindow: {
        new (options: BrowserWindowOptions): BrowserWindow;
        fromWebContents(contents: WebContents): BrowserWindow | null;
        getAllWindows(): BrowserWindow[];
    };
}
