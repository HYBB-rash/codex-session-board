import { reactive } from "vue";

interface InstallPromptEvent extends Event {
    prompt(): Promise<unknown>;
    readonly userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

declare global {
    interface WindowEventMap {
        beforeinstallprompt: InstallPromptEvent;
    }
}

export interface BoardInstall {
    available: boolean;
    prompt(): Promise<void>;
    dispose(): void;
}

function reportInstallError(signal: AbortSignal, message: string) {
    if (signal.aborted) return;
    const feedback = document.getElementById("feedback");
    if (!feedback) return;
    feedback.textContent = message;
    feedback.className = "feedback error";
}

function registerWorker(signal: AbortSignal) {
    if ("serviceWorker" in navigator) {
        navigator.serviceWorker
            .register("/sw.js", { scope: "/", updateViaCache: "none" })
            .catch(() => {
                reportInstallError(signal, "应用组件注册失败，请刷新重试。看板仍可正常使用。");
            });
    }
}

export function setupPwa(): BoardInstall | undefined {
    if (!window.isSecureContext || window.top !== window) return undefined;
    const lifetime = new AbortController();
    const options = { signal: lifetime.signal };
    const installed = window.matchMedia("(display-mode: standalone)");
    let installPrompt: InstallPromptEvent | null = null;
    const hideInstall = () => {
        installPrompt = null;
        install.available = false;
    };
    async function promptInstall() {
        const prompt = installPrompt;
        if (!prompt) return;
        hideInstall();
        try {
            await prompt.prompt();
            await prompt.userChoice;
        } catch {
            reportInstallError(lifetime.signal, "未能打开安装窗口，请从浏览器菜单安装会话看板。");
        }
    }
    const install = reactive<BoardInstall>({
        available: false,
        prompt: promptInstall,
        dispose: () => lifetime.abort(),
    });
    window.addEventListener(
        "beforeinstallprompt",
        (event) => {
            event.preventDefault();
            if (installed.matches) return;
            installPrompt = event;
            install.available = true;
        },
        options,
    );
    window.addEventListener("appinstalled", hideInstall, options);
    installed.addEventListener(
        "change",
        () => {
            if (installed.matches) hideInstall();
        },
        options,
    );
    registerWorker(lifetime.signal);
    return install;
}
