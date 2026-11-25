type PreviewRequest =
    | { action: "preview"; target: Record<string, FormDataEntryValue> }
    | { action: "restore" | "recheck"; targetId: string };
type SetupRequest =
    | PreviewRequest
    | { action: "status" }
    | { action: "decline" | "apply"; previewId: string };
type ResumableState = "approved" | "filesWritten" | "linkDelivered";
type SavedSetup =
    | { state: "registered"; summary: string; message: string }
    | {
          state: "declined" | ResumableState;
          targetId: string;
          previewId: string;
          summary: string;
          message: string;
      };
type SetupResult = (
    | SavedSetup
    | { state: "form"; saved?: SavedSetup[] }
    | { state: "selectAlias"; aliases: string[] }
    | {
          state: "preview";
          previewId: string;
          canApply: boolean;
          changes: { label: string; detail?: string }[];
      }
    | { state: "applied" }
) & { summary?: string; message?: string };

interface SshSetupPorts {
    request(path: "/api/ssh-setup", body: SetupRequest & { alias?: string }): Promise<unknown>;
    locked(): boolean;
    available(): boolean;
    native(): boolean;
    updateFilterLock(): void;
    announce(message: string): void;
    load(): Promise<void>;
}

function resumable(
    result: SetupResult,
): result is SetupResult & { state: ResumableState; previewId: string; targetId: string } {
    return (
        result.state === "approved" ||
        result.state === "filesWritten" ||
        result.state === "linkDelivered"
    );
}

function sshSetupElements(dialog: HTMLDialogElement) {
    const form = dialog.querySelector("form");
    if (!form) throw new Error("缺少 SSH 配置表单");
    function find<ElementType extends Element>(selector: string) {
        const element = form?.querySelector<ElementType>(selector);
        if (!element) throw new Error(`缺少 SSH 配置元素 ${selector}`);
        return element;
    }
    return {
        form,
        fields: find<HTMLFieldSetElement>("fieldset"),
        alias: find<HTMLSelectElement>("select"),
        aliasChoice: find<HTMLElement>("#ssh-setup-alias-choice"),
        previewPanel: find<HTMLElement>("#ssh-setup-preview"),
        target: find<HTMLElement>("#ssh-setup-target"),
        changes: find("#ssh-setup-changes"),
        notice: find("#ssh-setup-notice"),
        saved: find<HTMLElement>("#ssh-setup-saved"),
        close: find<HTMLButtonElement>("#ssh-setup-close"),
        decline: find<HTMLButtonElement>("#ssh-setup-decline"),
        restore: find<HTMLButtonElement>("#ssh-setup-restore"),
        recheck: find<HTMLButtonElement>("#ssh-setup-recheck"),
        next: find<HTMLButtonElement>("#ssh-setup-next"),
    };
}

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

export function mountBoardSshSetup(
    settings: HTMLButtonElement,
    dialog: HTMLDialogElement,
    ports: SshSetupPorts,
) {
    let setupResult: SetupResult = { state: "form" };
    let busy = false;
    let previewRequest: PreviewRequest | undefined;

    function showSaved(record: SavedSetup) {
        const item = document.createElement("p"),
            message = document.createElement("span");
        item.textContent = record.summary;
        message.textContent = record.message;
        item.append(" ", message);
        if (record.state !== "registered") {
            const button = document.createElement("button");
            button.type = "button";
            button.textContent = record.state === "declined" ? "重新启用" : "核对配置进度";
            button.disabled = busy;
            button.onclick = () => {
                setupResult = record;
                if (record.state === "declined")
                    void requestSetup({ action: "restore", targetId: record.targetId });
                else render();
            };
            item.append(" ", button);
        }
        return item;
    }

    function renderActions(current: SetupResult) {
        const { decline, restore, recheck, next, close } = elements;
        const preview = current.state === "preview";
        const continuing = resumable(current);
        decline.hidden = !preview;
        restore.hidden = current.state !== "declined";
        recheck.hidden = !continuing;
        next.hidden = !(
            preview ||
            continuing ||
            current.state === "form" ||
            current.state === "selectAlias"
        );
        next.textContent = preview ? "同意并配置" : continuing ? "继续配置" : "检查并预览";
        next.disabled = busy || (preview && current.canApply !== true);
        for (const button of [close, decline, restore, recheck]) button.disabled = busy;
    }

    function render() {
        const current = setupResult;
        const { aliasChoice, alias, fields, previewPanel, target, changes, notice, saved } =
            elements;
        const choosing = current.state === "selectAlias";
        const entering = current.state === "form";
        aliasChoice.hidden = !choosing;
        if (choosing && !busy)
            alias.replaceChildren(...current.aliases.map((name) => new Option(name, name)));
        alias.disabled = busy;
        fields.disabled = busy || !entering;
        fields.hidden = !entering;
        previewPanel.hidden = entering || choosing;
        target.textContent = current.summary || "";
        changes.replaceChildren(
            ...(current.state === "preview" ? current.changes.map(showChange) : []),
        );
        notice.textContent = current.message || "";
        const records = entering ? (current.saved ?? []) : [];
        saved.hidden = !records.length;
        saved.replaceChildren(...records.map((record) => showSaved(record)));
        renderActions(current);
    }

    function closeAfterApply(result: SetupResult) {
        if (result.state !== "applied") return;
        ports.announce(result.message || "");
        dialog.close();
    }

    function finishRequest() {
        busy = false;
        render();
        if (!dialog.open) void ports.load();
    }

    async function requestSetup(request: SetupRequest, selection?: string) {
        if (busy || !ports.available()) return;
        if (request.action === "preview" || "targetId" in request) previewRequest = request;
        busy = true;
        render();
        try {
            setupResult = (await ports.request(
                "/api/ssh-setup",
                selection ? { ...request, alias: selection } : request,
            )) as SetupResult;
            closeAfterApply(setupResult);
        } catch (error) {
            setupResult = {
                ...setupResult,
                message: error instanceof Error ? error.message : String(error),
            };
        } finally {
            finishRequest();
        }
    }

    function open() {
        if (ports.locked()) return;
        if (ports.native()) {
            void requestSetup({ action: "status" });
            return;
        }
        setupResult = { state: "form" };
        previewRequest = undefined;
        dialog.showModal();
        ports.updateFilterLock();
        void requestSetup({ action: "status" });
    }

    function submit(event: SubmitEvent) {
        const result = setupResult;
        const currentPreview = previewRequest;
        const { alias, form } = elements;
        event.preventDefault();
        if (resumable(result) || (result.state === "preview" && result.canApply === true))
            void requestSetup({ action: "apply", previewId: result.previewId });
        else if (result.state === "selectAlias" && currentPreview)
            void requestSetup(currentPreview, alias.value);
        else if (result.state === "form")
            void requestSetup({
                action: "preview",
                target: Object.fromEntries(new FormData(form)),
            });
    }

    function update() {
        settings.hidden = !ports.available();
        if (ports.native()) settings.textContent = "Codex 远程连接";
    }

    const elements = sshSetupElements(dialog);
    const { form, decline, restore, recheck, close } = elements;
    settings.onclick = () => open();
    form.onsubmit = (event) => submit(event);
    decline.onclick = () => {
        if (setupResult.state === "preview")
            void requestSetup({ action: "decline", previewId: setupResult.previewId });
    };
    restore.onclick = () => {
        if (setupResult.state === "declined")
            void requestSetup({ action: "restore", targetId: setupResult.targetId });
    };
    recheck.onclick = () => {
        if (resumable(setupResult))
            void requestSetup({ action: "recheck", targetId: setupResult.targetId });
    };
    close.onclick = () => dialog.close();
    dialog.addEventListener("cancel", (event) => {
        if (busy) event.preventDefault();
    });
    dialog.addEventListener("close", () => {
        ports.updateFilterLock();
        void ports.load();
    });

    return {
        update,
        get openDialog() {
            return dialog.open;
        },
    };
}
