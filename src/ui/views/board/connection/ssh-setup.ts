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

export function mountBoardSshSetup(
    settings: HTMLButtonElement,
    dialog: HTMLDialogElement,
    ports: SshSetupPorts,
) {
    return new BoardSshSetup(settings, dialog, ports);
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

class BoardSshSetup {
    result: SetupResult = { state: "form" };
    busy = false;
    private currentPreview: PreviewRequest | undefined;
    private readonly elements: ReturnType<typeof sshSetupElements>;

    constructor(
        private readonly settings: HTMLButtonElement,
        private readonly dialog: HTMLDialogElement,
        private readonly ports: SshSetupPorts,
    ) {
        this.elements = sshSetupElements(dialog);
        const { form, decline, restore, recheck, close } = this.elements;
        settings.onclick = () => this.open();
        form.onsubmit = (event) => this.submit(event);
        decline.onclick = () => {
            if (this.result.state === "preview")
                void this.requestSetup({ action: "decline", previewId: this.result.previewId });
        };
        restore.onclick = () => {
            if (this.result.state === "declined")
                void this.requestSetup({ action: "restore", targetId: this.result.targetId });
        };
        recheck.onclick = () => {
            if (resumable(this.result))
                void this.requestSetup({ action: "recheck", targetId: this.result.targetId });
        };
        close.onclick = () => dialog.close();
        dialog.addEventListener("cancel", (event) => {
            if (this.busy) event.preventDefault();
        });
        dialog.addEventListener("close", () => {
            ports.updateFilterLock();
            void ports.load();
        });
    }

    private showSaved(record: SavedSetup) {
        const item = document.createElement("p"),
            message = document.createElement("span");
        item.textContent = record.summary;
        message.textContent = record.message;
        item.append(" ", message);
        if (record.state !== "registered") {
            const button = document.createElement("button");
            button.type = "button";
            button.textContent = record.state === "declined" ? "重新启用" : "核对配置进度";
            button.disabled = this.busy;
            button.onclick = () => {
                this.result = record;
                if (record.state === "declined")
                    void this.requestSetup({ action: "restore", targetId: record.targetId });
                else this.render();
            };
            item.append(" ", button);
        }
        return item;
    }

    private render() {
        const { busy, result: current } = this;
        const {
            aliasChoice,
            alias,
            fields,
            previewPanel,
            target,
            changes,
            notice,
            decline,
            restore,
            saved,
            recheck,
            next,
            close,
        } = this.elements;
        const preview = current.state === "preview";
        const choosing = current.state === "selectAlias";
        const entering = current.state === "form";
        const continuing = resumable(current);
        aliasChoice.hidden = !choosing;
        if (choosing && !busy)
            alias.replaceChildren(...current.aliases.map((name) => new Option(name, name)));
        alias.disabled = busy;
        fields.disabled = busy || !entering;
        fields.hidden = !entering;
        previewPanel.hidden = entering || choosing;
        target.textContent = current.summary || "";
        changes.replaceChildren(...(preview ? current.changes.map(showChange) : []));
        notice.textContent = current.message || "";
        decline.hidden = !preview;
        restore.hidden = current.state !== "declined";
        const records = entering ? (current.saved ?? []) : [];
        saved.hidden = !records.length;
        saved.replaceChildren(...records.map((record) => this.showSaved(record)));
        recheck.hidden = !continuing;
        next.hidden = !entering && !preview && !continuing && !choosing;
        next.textContent = preview ? "同意并配置" : continuing ? "继续配置" : "检查并预览";
        next.disabled = busy || (preview && current.canApply !== true);
        for (const button of [close, decline, restore, recheck]) button.disabled = busy;
    }

    private async requestSetup(request: SetupRequest, selection?: string) {
        const { ports, dialog } = this;
        if (this.busy || !ports.available()) return;
        if (
            request.action === "preview" ||
            request.action === "restore" ||
            request.action === "recheck"
        )
            this.currentPreview = request;
        this.busy = true;
        this.render();
        try {
            this.result = (await ports.request(
                "/api/ssh-setup",
                selection ? { ...request, alias: selection } : request,
            )) as SetupResult;
            if (this.result.state === "applied") {
                ports.announce(this.result.message || "");
                dialog.close();
            }
        } catch (error) {
            this.result = {
                ...this.result,
                message: error instanceof Error ? error.message : String(error),
            };
        } finally {
            this.busy = false;
            this.render();
            if (!dialog.open) void ports.load();
        }
    }

    private open() {
        const { ports, dialog } = this;
        if (ports.locked()) return;
        if (ports.native()) {
            void this.requestSetup({ action: "status" });
            return;
        }
        this.result = { state: "form" };
        this.currentPreview = undefined;
        dialog.showModal();
        ports.updateFilterLock();
        void this.requestSetup({ action: "status" });
    }

    private submit(event: SubmitEvent) {
        const { result, currentPreview } = this;
        const { alias, form } = this.elements;
        event.preventDefault();
        if (resumable(result) || (result.state === "preview" && result.canApply === true))
            void this.requestSetup({ action: "apply", previewId: result.previewId });
        else if (result.state === "selectAlias" && currentPreview)
            void this.requestSetup(currentPreview, alias.value);
        else if (result.state === "form")
            void this.requestSetup({
                action: "preview",
                target: Object.fromEntries(new FormData(form)),
            });
    }

    update() {
        const { settings, ports } = this;
        settings.hidden = !ports.available();
        if (ports.native()) settings.textContent = "Codex 远程连接";
    }

    get openDialog() {
        return this.dialog.open;
    }
}
