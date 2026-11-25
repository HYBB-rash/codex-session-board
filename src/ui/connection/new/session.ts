import type { CatalogProject } from "../../browse/scope";
import type { BoardSnapshot } from "../../controller";

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
    const draftRequests = new Map<string, string>();
    const nativeOperations = new Map<string, string>();
    let recoveryDraft: DraftRequest | undefined;
    let consent:
        | {
              projectId: string | null;
              operationId: string | undefined;
              requestId: string | undefined;
              token: string;
          }
        | undefined;

    function canCreateProject(project?: CatalogProject) {
        if (!project || project.canCreate === false || project.pendingDeletion === true)
            return false;
        if (!targetProject(project)) return true;
        const host = ports
            .snapshot()
            .health.remoteConnections?.find((host) => host.hostId === project.hostId);
        return host?.connected === true && host.catalog?.cached === false;
    }

    function nativeOperation(projectId: string) {
        const key = `native-draft-operation:${projectId}`;
        let operation = nativeOperations.get(projectId);
        try {
            operation ||= sessionStorage.getItem(key) ?? undefined;
        } catch {}
        operation ||= crypto.randomUUID();
        nativeOperations.set(projectId, operation);
        try {
            sessionStorage.setItem(key, operation);
        } catch {}
        return operation;
    }

    function clearNativeOperation(projectId: string) {
        nativeOperations.delete(projectId);
        try {
            sessionStorage.removeItem(`native-draft-operation:${projectId}`);
        } catch {}
    }

    function recover(draft: DraftRequest, reason: string) {
        const { recoveryMessage, recoveryDialog } = elements;
        recoveryDraft = draft;
        recoveryMessage.textContent = reason;
        recoveryDialog.showModal();
    }

    function draftInput(
        projectId: string | null,
        project: CatalogProject | undefined,
        operationId?: string,
        consentToken?: string,
        requestId?: string,
    ): DraftInput {
        const body: DraftInput = {};
        if (consentToken) body.consent = consentToken;
        if (projectId === null) return body;
        body.projectId = projectId;
        if (projectId.startsWith("target:") || targetProject(project) || operationId) {
            body.operationId = operationId || nativeOperation(projectId);
        } else {
            const legacyRequest = requestId || draftRequests.get(projectId);
            if (legacyRequest) body.requestId = legacyRequest;
        }
        return body;
    }

    function reportCreateFailure(cause: unknown, projectId: string | null, body: DraftInput) {
        const message = cause instanceof Error ? cause.message : String(cause);
        if (projectId !== null && body.operationId)
            recover(
                { projectId, operationId: body.operationId },
                `新建尚未完成，请核对客户端和目标后重试。${message}`,
            );
        ports.announce(message, true);
    }

    async function create(
        projectId: string | null,
        operationId?: string,
        consentToken?: string,
        requestId?: string,
    ) {
        const { button } = elements;
        if (ports.creating() || ports.dragActive() || ports.saving() || ports.editorOpen()) return;
        const project = ports.snapshot().projectCatalog.items.find((item) => item.id === projectId);
        if (project && !canCreateProject(project)) {
            ports.announce(project.createError || unavailable, true);
            return;
        }
        const body = draftInput(projectId, project, operationId, consentToken, requestId);
        ports.setCreating(true);
        button.disabled = true;
        ports.invalidateSnapshot();
        ports.render();
        try {
            const result = (await ports.request("/api/new", body)) as NewSessionResult;
            receive(result, projectId, body, project);
        } catch (cause) {
            reportCreateFailure(cause, projectId, body);
        } finally {
            ports.setCreating(false);
            button.disabled = false;
            ports.render();
            await ports.load();
        }
    }

    function recoverPending(projectId: string, result: NewSessionResult, body: DraftInput) {
        const draft = body.operationId
            ? { projectId, operationId: body.operationId }
            : { projectId, requestId: result.requestId || body.requestId };
        if (draft.requestId) draftRequests.set(projectId, draft.requestId);
        recover(draft, result.error ?? "");
    }

    function clearDraft(projectId: string | null, body: DraftInput) {
        if (projectId === null) return;
        if (body.operationId) clearNativeOperation(projectId);
        draftRequests.delete(projectId);
    }

    function receive(
        result: NewSessionResult,
        projectId: string | null,
        body: DraftInput,
        project?: CatalogProject,
    ) {
        if (result.consent) {
            showConsent(result.consent, projectId, body);
            return;
        }
        if (projectId !== null && result.pending) {
            recoverPending(projectId, result, body);
            return;
        }
        if (result.registration) {
            ports.announce(
                "请在 Codex 确认目标目录。登记后若未进入输入页，回到看板重新打开同一项目。",
            );
            return;
        }
        clearDraft(projectId, body);
        const remote = body.operationId || body.requestId || project?.remote;
        ports.announce(
            remote
                ? "已请求打开远程原生输入页。请核对主机和目录，发送第一句话后才会创建会话；回到看板刷新可确认项目归属。"
                : "已发送新建会话页面打开请求。",
        );
    }

    function showConsent(
        { token, summary, changes }: ConsentPreview,
        projectId: string | null,
        body: DraftInput,
    ) {
        const { consentTarget, consentChanges, consentDialog } = elements;
        consent = {
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

    function loadAfterClose() {
        ports.updateFilterLock();
        void ports.load();
    }

    function bindConsent() {
        const { consentForm, consentDialog, consentCancel } = elements;
        consentForm.onsubmit = (event) => {
            event.preventDefault();
            const accepted = consent;
            if (!consentDialog.open || !accepted) return;
            consentDialog.close();
            void create(
                accepted.projectId,
                accepted.operationId,
                accepted.token,
                accepted.requestId,
            );
        };
        consentCancel.onclick = () => consentDialog.close();
        consentDialog.addEventListener("close", () => {
            if (consentDialog.open) return;
            consent = undefined;
            loadAfterClose();
        });
    }

    function bindRecovery() {
        const { recoveryLater, recoveryDialog, recoveryRetry, recoveryRestart } = elements;
        recoveryLater.onclick = () => recoveryDialog.close();
        recoveryDialog.addEventListener("close", () => loadAfterClose());
        recoveryRetry.onclick = () => {
            if (!recoveryDraft || !recoveryDialog.open) return;
            recoveryDialog.close();
            const { projectId, operationId, requestId } = recoveryDraft;
            void create(projectId, operationId, undefined, requestId);
        };
        recoveryRestart.onclick = () => restart();
    }

    async function restart() {
        const recovering = recoveryDraft;
        const { recoveryDialog, recoveryRestart, recoveryMessage } = elements;
        if (ports.saving() || ports.creating() || !recovering || !recoveryDialog.open) return;
        const { projectId, operationId, requestId } = recovering;
        recoveryRestart.disabled = true;
        ports.setCreating(true);
        ports.invalidateSnapshot();
        try {
            if (operationId) clearNativeOperation(projectId);
            else {
                await ports.request("/api/new/reset", { projectId, requestId, confirmed: true });
                draftRequests.delete(projectId);
            }
            recoveryDialog.close();
            ports.setCreating(false);
            await create(projectId);
        } catch (cause) {
            recoveryMessage.textContent = cause instanceof Error ? cause.message : String(cause);
        } finally {
            ports.setCreating(false);
            recoveryRestart.disabled = false;
        }
    }

    function update(snapshot: BoardSnapshot) {
        for (const draft of snapshot.pendingDrafts ?? [])
            draftRequests.set(draft.project, draft.requestId);
    }

    function hasPendingDraft(projectId: string) {
        return draftRequests.has(projectId);
    }

    bindConsent();
    bindRecovery();

    return {
        canCreateProject,
        create,
        update,
        get openDialog() {
            const { consentDialog, recoveryDialog } = elements;
            return consentDialog.open || recoveryDialog.open;
        },
        hasPendingDraft,
    };
}
