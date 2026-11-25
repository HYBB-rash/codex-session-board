interface UpdateStatus {
  available: boolean;
  busy?: boolean;
  phase?: string;
  version?: string;
  runningVersion?: string;
  error?: string;
  failedStage?: string;
}

const pendingStages: Record<string, string> = {
  fetching: "正在获取候选包",
  verifying: "正在校验候选包",
  installing: "正在安装候选版",
  reloading: "正在重载并读回看板",
};

function describeUpdate(result: UpdateStatus): string {
  const version = result.version ? ` ${result.version}` : "";
  const detail = result.error ? ` 原因：${result.error}` : "";
  const running = ` 当前运行版本 ${result.runningVersion}。`;
  const stages: Record<string, string> = {
    up_to_date: `已是最新版${version}。`,
    active: `新版${version} 已生效，看板已恢复。`,
    awaiting_desktop_reload: `${version.trim()} 已安装，待重载。请完全退出并重新启动 Codex，再打开看板。也可点击「更新」重试重载。${running}${detail}`,
    reopen_board: `${version.trim()} 已安装，已请求重载。请关闭并重新打开看板确认新版。如果仍是旧版，请重启 Codex。${running}`,
    failed: result.failedStage === "installing"
      ? `安装${version}失败，未能确认新包安装完成。${running} 请点击「更新」重试安装。${detail}`
      : result.failedStage === "reloading"
      ? `已安装${version}，重载结果未确认。${running} 请重启 Codex 后重新打开看板。${detail}`
      : `更新失败，原安装未更改。请检查网络或包校验报错，再点击「更新」。${detail}`,
  };
  const phase = result.phase ?? "";
  if (Object.hasOwn(pendingStages, phase)) return `${pendingStages[phase]}${version}…`;
  const stage = Object.hasOwn(stages, phase) ? stages[phase] : undefined;
  return stage ?? (result.busy ? "已有安装任务正在运行…" : "");
}

export function mountPluginUpdate(button: HTMLButtonElement, status: HTMLElement,
  request: (path: "/api/plugin-update", body?: object) => Promise<unknown>) {
  let reading = false;
  let starting = false;
  let generation = 0;
  let available: boolean | undefined;

  function show(message: string) {
    status.textContent = message;
    status.hidden = !message;
  }

  async function read(start = false) {
    if (starting || available === false || (!start && reading)) return;
    if (start) {
      starting = true;
      generation++;
      button.disabled = true;
    } else reading = true;
    const requested = generation;
    try {
      const result = await request("/api/plugin-update", start ? {} : undefined) as UpdateStatus;
      if (requested !== generation) return;
      available = result.available;
      button.hidden = !available;
      button.disabled = result.busy === true;
      show(describeUpdate(result));
    } catch (error) {
      if (requested === generation && (available || start)) {
        const detail = error instanceof Error ? error.message : String(error);
        show(`暂时无法确认更新状态。请重新打开看板后核对结果。 ${detail}`);
        button.disabled = false;
      }
    } finally {
      if (start) starting = false;
      else reading = false;
    }
  }

  button.onclick = () => read(true);
  void read();
  const timer = window.setInterval(read, 2000);
  return () => {
    window.clearInterval(timer);
    generation++;
  };
}
