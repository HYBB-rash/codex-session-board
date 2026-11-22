type PreviewRequest = { action: "preview"; target: Record<string, FormDataEntryValue> }
  | { action: "restore" | "recheck"; targetId: string };
type SetupRequest = PreviewRequest | { action: "status" } | { action: "decline" | "apply"; previewId: string };
type ResumableState = "approved" | "filesWritten" | "linkDelivered";
type SavedSetup = { state: "registered"; summary: string; message: string }
  | { state: "declined" | ResumableState; targetId: string; previewId: string; summary: string; message: string };
type SetupResult = (SavedSetup
  | { state: "form"; saved?: SavedSetup[] }
  | { state: "selectAlias"; aliases: string[] }
  | { state: "preview"; previewId: string; canApply: boolean; changes: { label: string; detail?: string }[] }
  | { state: "applied" }) & { summary?: string; message?: string };

interface SshSetupPorts {
  request(path: "/api/ssh-setup", body: SetupRequest & { alias?: string }): Promise<unknown>;
  locked(): boolean;
  available(): boolean;
  native(): boolean;
  updateFilterLock(): void;
  announce(message: string): void;
  load(): Promise<void>;
}

function resumable(result: SetupResult): result is SetupResult & { state: ResumableState; previewId: string; targetId: string } {
  return result.state === "approved" || result.state === "filesWritten" || result.state === "linkDelivered";
}

export function mountBoardSshSetup(settings: HTMLButtonElement, dialog: HTMLDialogElement, ports: SshSetupPorts) {
  const form = dialog.querySelector("form")!;
  const fields = form.querySelector("fieldset")!;
  const alias = form.querySelector("select")!;
  const aliasChoice = form.querySelector<HTMLElement>("#ssh-setup-alias-choice")!;
  const previewPanel = form.querySelector<HTMLElement>("#ssh-setup-preview")!;
  const target = form.querySelector<HTMLElement>("#ssh-setup-target")!;
  const changes = form.querySelector("#ssh-setup-changes")!;
  const notice = form.querySelector("#ssh-setup-notice")!;
  const saved = form.querySelector<HTMLElement>("#ssh-setup-saved")!;
  const close = form.querySelector<HTMLButtonElement>("#ssh-setup-close")!;
  const decline = form.querySelector<HTMLButtonElement>("#ssh-setup-decline")!;
  const restore = form.querySelector<HTMLButtonElement>("#ssh-setup-restore")!;
  const recheck = form.querySelector<HTMLButtonElement>("#ssh-setup-recheck")!;
  const next = form.querySelector<HTMLButtonElement>("#ssh-setup-next")!;
  let result: SetupResult = { state: "form" };
  let busy = false;
  let currentPreview: PreviewRequest | undefined;

  function showChange(change: { label: string; detail?: string }) {
    const item = document.createElement("li");
    item.textContent = change.label;
    if (change.detail) {
      const diff = document.createElement("pre");
      diff.textContent = change.detail;
      item.append(diff);
    }
    return item;
  }

  function showSaved(record: SavedSetup) {
    const item = document.createElement("p"), message = document.createElement("span");
    item.textContent = record.summary;
    message.textContent = record.message;
    item.append(" ", message);
    if (record.state !== "registered") {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = record.state === "declined" ? "重新启用" : "核对配置进度";
      button.disabled = busy;
      button.onclick = () => {
        result = record;
        if (record.state === "declined") void requestSetup({ action: "restore", targetId: record.targetId });
        else render();
      };
      item.append(" ", button);
    }
    return item;
  }

  function render() {
    const current = result;
    const preview = current.state === "preview";
    const choosing = current.state === "selectAlias";
    const entering = current.state === "form";
    const continuing = resumable(current);
    aliasChoice.hidden = !choosing;
    if (choosing && !busy) alias.replaceChildren(...current.aliases.map(name => new Option(name, name)));
    alias.disabled = busy;
    fields.disabled = busy || !entering;
    fields.hidden = !entering;
    previewPanel.hidden = entering || choosing;
    target.textContent = current.summary || "";
    changes.replaceChildren(...(preview ? current.changes.map(showChange) : []));
    notice.textContent = current.message || "";
    decline.hidden = !preview;
    restore.hidden = current.state !== "declined";
    const records = entering ? current.saved ?? [] : [];
    saved.hidden = !records.length;
    saved.replaceChildren(...records.map(showSaved));
    recheck.hidden = !continuing;
    next.hidden = !entering && !preview && !continuing && !choosing;
    next.textContent = preview ? "同意并配置" : continuing ? "继续配置" : "检查并预览";
    next.disabled = busy || preview && current.canApply !== true;
    for (const button of [close, decline, restore, recheck]) button.disabled = busy;
  }

  async function requestSetup(request: SetupRequest, selection?: string) {
    if (busy || !ports.available()) return;
    if (request.action === "preview" || request.action === "restore" || request.action === "recheck")
      currentPreview = request;
    busy = true;
    render();
    try {
      result = await ports.request("/api/ssh-setup", selection ? { ...request, alias: selection } : request) as SetupResult;
      if (result.state === "applied") {
        ports.announce(result.message || "");
        dialog.close();
      }
    } catch (error) {
      result = { ...result, message: error instanceof Error ? error.message : String(error) };
    } finally {
      busy = false;
      render();
      if (!dialog.open) void ports.load();
    }
  }

  settings.onclick = () => {
    if (ports.locked()) return;
    if (ports.native()) {
      void requestSetup({ action: "status" });
      return;
    }
    result = { state: "form" };
    currentPreview = undefined;
    dialog.showModal();
    ports.updateFilterLock();
    void requestSetup({ action: "status" });
  };
  form.onsubmit = event => {
    event.preventDefault();
    if (resumable(result) || result.state === "preview" && result.canApply === true)
      void requestSetup({ action: "apply", previewId: result.previewId });
    else if (result.state === "selectAlias" && currentPreview)
      void requestSetup(currentPreview, alias.value);
    else if (result.state === "form")
      void requestSetup({ action: "preview", target: Object.fromEntries(new FormData(form)) });
  };
  decline.onclick = () => {
    if (result.state === "preview") void requestSetup({ action: "decline", previewId: result.previewId });
  };
  restore.onclick = () => {
    if (result.state === "declined") void requestSetup({ action: "restore", targetId: result.targetId });
  };
  recheck.onclick = () => {
    if (resumable(result)) void requestSetup({ action: "recheck", targetId: result.targetId });
  };
  close.onclick = () => dialog.close();
  dialog.addEventListener("cancel", event => { if (busy) event.preventDefault(); });
  dialog.addEventListener("close", () => { ports.updateFilterLock(); void ports.load(); });
  return {
    update: () => {
      settings.hidden = !ports.available();
      if (ports.native()) settings.textContent = "Codex 远程连接";
    },
    get openDialog() { return dialog.open; },
    get result() { return result; },
    get busy() { return busy; },
  };
}
