interface InstallPromptEvent extends Event {
    prompt(): Promise<unknown>;
    readonly userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

declare global {
    interface WindowEventMap {
        beforeinstallprompt: InstallPromptEvent;
    }
}

function setupPwa(button: HTMLElement) {
    const installed = window.matchMedia("(display-mode: standalone)");
    let installPrompt: InstallPromptEvent | null = null;
    const report = (message: string) => {
        const feedback = document.getElementById("feedback");
        if (!feedback) return;
        feedback.textContent = message;
        feedback.className = "feedback error";
    };
    const hideInstall = () => {
        installPrompt = null;
        button.hidden = true;
    };
    window.addEventListener("beforeinstallprompt", (event) => {
        event.preventDefault();
        if (installed.matches) return;
        installPrompt = event;
        button.hidden = false;
    });
    window.addEventListener("appinstalled", hideInstall);
    installed.addEventListener("change", () => {
        if (installed.matches) hideInstall();
    });
    button.addEventListener("click", async () => {
        const prompt = installPrompt;
        if (!prompt) return;
        hideInstall();
        try {
            await prompt.prompt();
            await prompt.userChoice;
        } catch {
            report("未能打开安装窗口，请从浏览器菜单安装会话看板。");
        }
    });
    if ("serviceWorker" in navigator) {
        navigator.serviceWorker
            .register("/sw.js", { scope: "/", updateViaCache: "none" })
            .catch(() => report("应用组件注册失败，请刷新重试。看板仍可正常使用。"));
    }
}

if (window.isSecureContext && window.top === window) {
    const button = document.getElementById("install-app");
    if (button) setupPwa(button);
}

export {};
