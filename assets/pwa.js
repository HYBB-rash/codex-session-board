(() => {
  if (!window.isSecureContext || window.top !== window) return;
  const installed = window.matchMedia("(display-mode: standalone)");
  const button = document.createElement("button");
  button.id = "install-app";
  button.type = "button";
  button.textContent = "安装应用";
  button.hidden = true;
  document.querySelector(".tools").append(button);
  let installPrompt = null;
  const feedback = message => {
    const node = document.getElementById("feedback");
    node.textContent = message;
    node.className = "feedback error";
  };
  const hideInstall = () => {
    installPrompt = null;
    button.hidden = true;
  };
  window.addEventListener("beforeinstallprompt", event => {
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
      feedback("未能打开安装窗口，请从浏览器菜单安装会话看板。");
    }
  });
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("/sw.js", {scope: "/", updateViaCache: "none"})
      .catch(() => feedback("应用组件注册失败，请刷新重试。看板仍可正常使用。"));
  }
})();
