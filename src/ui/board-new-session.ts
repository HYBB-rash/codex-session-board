import type { CatalogProject } from "./board-browse";
import type { BoardSnapshot } from "./board-start";

interface NewSessionElements {
    button: HTMLButtonElement;
    consentDialog: HTMLDialogElement;
    consentForm: HTMLFormElement;
    consentTarget: HTMLElement;
    consentChanges: HTMLElement;
    consentCancel: HTMLButtonElement;
    recoveryDialog: HTMLDialogElement;
    recoveryMessage: HTMLElement;
    recoveryLater: HTMLButtonElement;
    recoveryRetry: HTMLButtonElement;
    recoveryRestart: HTMLButtonElement;
}

interface DraftRequest {
    projectId: string;
    requestId?: string | undefined;
    operationId?: string;
}

type DraftInput = Partial<DraftRequest> & { consent?: string };

interface ConsentPreview {
    token: string;
    summary: string;
    changes: string[];
}

interface NewSessionResult {
    ok?: boolean;
    pending?: boolean;
    registration?: boolean;
    requestId?: string;
    error?: string;
    consent?: ConsentPreview;
}

interface NewSessionPorts {
    snapshot(): BoardSnapshot;
    saving(): boolean;
    creating(): boolean;
    setCreating(value: boolean): void;
    dragActive(): boolean;
    editorOpen(): boolean;
    updateFilterLock(): void;
    invalidateSnapshot(): void;
    request(
        path: "/api/new" | "/api/new/reset",
        body: DraftInput & { confirmed?: true },
    ): Promise<unknown>;
    announce(message: string, error?: boolean): void;
    render(): void;
    load(): Promise<void>;
}

const unavailable = "当前项目暂不可新建，请刷新并核对目标主机。";
const targetProject = (project?: CatalogProject) =>
    project?.origin === "targetServer" || project?.origin === "targetLegacy";

export function mountBoardNewSession(elements: NewSessionElements, ports: NewSessionPorts) {
    return new BoardNewSession(elements, ports);
}

class BoardNewSession {
    private readonly draftRequests = new Map<string, string>();
    private readonly nativeOperations = new Map<string, string>();
    private recovering: DraftRequest | undefined;
    private consent:
        | {
              projectId: string | null;
              operationId: string | undefined;
              requestId: string | undefined;
              token: string;
          }
        | undefined;

    constructor(
        private readonly elements: NewSessionElements,
        private readonly ports: NewSessionPorts,
    ) {
        this.bindConsent();
        this.bindRecovery();
    }

    readonly canCreateProject = (project?: CatalogProject) => {
        if (!project || project.canCreate === false || project.pendingDeletion === true)
            return false;
        if (!targetProject(project)) return true;
        const host = this.ports
            .snapshot()
            .health.remoteConnections?.find((host) => host.hostId === project.hostId);
        return host?.connected === true && host.catalog?.cached === false;
    };

    private nativeOperation(projectId: string) {
        const key = `native-draft-operation:${projectId}`;
        let operation = this.nativeOperations.get(projectId);
        try {
            operation ||= sessionStorage.getItem(key) ?? undefined;
        } catch {}
        operation ||= crypto.randomUUID();
        this.nativeOperations.set(projectId, operation);
        try {
            sessionStorage.setItem(key, operation);
        } catch {}
        return operation;
    }

    private clearNativeOperation(projectId: string) {
        this.nativeOperations.delete(projectId);
        try {
            sessionStorage.removeItem(`native-draft-operation:${projectId}`);
        } catch {}
    }

    private recover(draft: DraftRequest, reason: string) {
        const { recoveryMessage, recoveryDialog } = this.elements;
        this.recovering = draft;
        recoveryMessage.textContent = reason;
        recoveryDialog.showModal();
    }

    readonly create = async (
        projectId: string | null,
        operationId?: string,
        consentToken?: string,
        requestId?: string,
    ) => {
        const { ports } = this;
        const { button } = this.elements;
        if (ports.creating() || ports.dragActive() || ports.saving() || ports.editorOpen()) return;
        const project = ports.snapshot().projectCatalog.items.find((item) => item.id === projectId);
        if (project && !this.canCreateProject(project)) {
            ports.announce(project.createError || unavailable, true);
            return;
        }
        const body: DraftInput = {};
        if (projectId !== null) {
            body.projectId = projectId;
            if (projectId.startsWith("target:") || targetProject(project) || operationId)
                body.operationId = operationId || this.nativeOperation(projectId);
            else {
                const legacyRequest = requestId || this.draftRequests.get(projectId);
                if (legacyRequest) body.requestId = legacyRequest;
            }
        }
        if (consentToken) body.consent = consentToken;
        ports.setCreating(true);
        button.disabled = true;
        ports.invalidateSnapshot();
        ports.render();
        try {
            const result = (await ports.request("/api/new", body)) as NewSessionResult;
            this.receive(result, projectId, body, project);
        } catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            if (projectId !== null && body.operationId)
                this.recover(
                    { projectId, operationId: body.operationId },
                    `新建尚未完成，请核对客户端和目标后重试。${message}`,
                );
            ports.announce(message, true);
        } finally {
            ports.setCreating(false);
            button.disabled = false;
            ports.render();
            await ports.load();
        }
    };

    private receive(
        result: NewSessionResult,
        projectId: string | null,
        body: DraftInput,
        project?: CatalogProject,
    ) {
        if (result.consent) {
            this.showConsent(result.consent, projectId, body);
            return;
        }
        if (projectId !== null && result.pending) {
            const draft = body.operationId
                ? { projectId, operationId: body.operationId }
                : { projectId, requestId: result.requestId || body.requestId };
            if (draft.requestId) this.draftRequests.set(projectId, draft.requestId);
            this.recover(draft, result.error ?? "");
            return;
        }
        if (result.registration) {
            this.ports.announce(
                "请在 Codex 确认目标目录。登记后若未进入输入页，回到看板重新打开同一项目。",
            );
            return;
        }
        if (projectId !== null) {
            if (body.operationId) this.clearNativeOperation(projectId);
            this.draftRequests.delete(projectId);
        }
        const remote = body.operationId || body.requestId || project?.remote;
        this.ports.announce(
            remote
                ? "已请求打开远程原生输入页。请核对主机和目录，发送第一句话后才会创建会话；回到看板刷新可确认项目归属。"
                : "已发送新建会话页面打开请求。",
        );
    }

    private showConsent(
        { token, summary, changes }: ConsentPreview,
        projectId: string | null,
        body: DraftInput,
    ) {
        const { consentTarget, consentChanges, consentDialog } = this.elements;
        this.consent = {
            projectId,
            operationId: body.operationId,
            requestId: body.requestId,
            token,
        };
        consentTarget.textContent = summary;
        consentChanges.replaceChildren(
            ...changes.map((change) => {
                const item = document.createElement("li");
                item.textContent = change;
                return item;
            }),
        );
        consentDialog.showModal();
    }

    private loadAfterClose() {
        this.ports.updateFilterLock();
        void this.ports.load();
    }

    private bindConsent() {
        const { consentForm, consentDialog, consentCancel } = this.elements;
        consentForm.onsubmit = (event) => {
            event.preventDefault();
            const accepted = this.consent;
            if (!consentDialog.open || !accepted) return;
            consentDialog.close();
            void this.create(
                accepted.projectId,
                accepted.operationId,
                accepted.token,
                accepted.requestId,
            );
        };
        consentCancel.onclick = () => consentDialog.close();
        consentDialog.addEventListener("close", () => {
            if (consentDialog.open) return;
            this.consent = undefined;
            this.loadAfterClose();
        });
    }

    private bindRecovery() {
        const { recoveryLater, recoveryDialog, recoveryRetry, recoveryRestart } = this.elements;
        recoveryLater.onclick = () => recoveryDialog.close();
        recoveryDialog.addEventListener("close", () => this.loadAfterClose());
        recoveryRetry.onclick = () => {
            if (!this.recovering || !recoveryDialog.open) return;
            recoveryDialog.close();
            const { projectId, operationId, requestId } = this.recovering;
            void this.create(projectId, operationId, undefined, requestId);
        };
        recoveryRestart.onclick = () => this.restart();
    }

    private async restart() {
        const { ports, recovering } = this;
        const { recoveryDialog, recoveryRestart, recoveryMessage } = this.elements;
        if (ports.saving() || ports.creating() || !recovering || !recoveryDialog.open) return;
        const { projectId, operationId, requestId } = recovering;
        recoveryRestart.disabled = true;
        ports.setCreating(true);
        ports.invalidateSnapshot();
        try {
            if (operationId) this.clearNativeOperation(projectId);
            else {
                await ports.request("/api/new/reset", { projectId, requestId, confirmed: true });
                this.draftRequests.delete(projectId);
            }
            recoveryDialog.close();
            ports.setCreating(false);
            await this.create(projectId);
        } catch (cause) {
            recoveryMessage.textContent = cause instanceof Error ? cause.message : String(cause);
        } finally {
            ports.setCreating(false);
            recoveryRestart.disabled = false;
        }
    }

    update(snapshot: BoardSnapshot) {
        for (const draft of snapshot.pendingDrafts ?? [])
            this.draftRequests.set(draft.project, draft.requestId);
    }

    get openDialog() {
        const { consentDialog, recoveryDialog } = this.elements;
        return consentDialog.open || recoveryDialog.open;
    }

    readonly hasPendingDraft = (projectId: string) => this.draftRequests.has(projectId);
}
