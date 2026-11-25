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
  request(path: "/api/new" | "/api/new/reset", body: DraftInput & { confirmed?: true }): Promise<unknown>;
  announce(message: string, error?: boolean): void;
  render(): void;
  load(): Promise<void>;
}

const unavailable = "当前项目暂不可新建，请刷新并核对目标主机。";
const targetProject = (project?: CatalogProject) => project?.origin === "targetServer" || project?.origin === "targetLegacy";

export function mountBoardNewSession(elements: NewSessionElements, ports: NewSessionPorts) {
  const { button, consentDialog, recoveryDialog, recoveryMessage, recoveryRestart } = elements;
  const { snapshot, render, load, announce, updateFilterLock } = ports;
  const draftRequests = new Map<string, string>();
  const nativeOperations = new Map<string, string>();
  let recovering: DraftRequest | undefined;
  let consent: { projectId: string | null; operationId: string | undefined; requestId: string | undefined; token: string } | undefined;

  function canCreateProject(project?: CatalogProject) {
    if (!project || project.canCreate === false || project.pendingDeletion === true) return false;
    if (!targetProject(project)) return true;
    const host = snapshot().health.remoteConnections?.find(host => host.hostId === project.hostId);
    return host?.connected === true && host.catalog?.cached === false;
  }

  function nativeOperation(projectId: string) {
    const key = "native-draft-operation:" + projectId;
    let operation = nativeOperations.get(projectId);
    try { operation ||= sessionStorage.getItem(key) ?? undefined; } catch {}
    operation ||= crypto.randomUUID();
    nativeOperations.set(projectId, operation);
    try { sessionStorage.setItem(key, operation); } catch {}
    return operation;
  }

  function clearNativeOperation(projectId: string) {
    nativeOperations.delete(projectId);
    try { sessionStorage.removeItem("native-draft-operation:" + projectId); } catch {}
  }

  function recover(draft: DraftRequest, reason: string) {
    recovering = draft;
    recoveryMessage.textContent = reason;
    recoveryDialog.showModal();
  }

  async function create(projectId: string | null, operationId?: string, consentToken?: string, requestId?: string) {
    if (ports.creating() || ports.dragActive() || ports.saving() || ports.editorOpen()) return;
    const project = snapshot().projectCatalog.items.find(item => item.id === projectId);
    if (project && !canCreateProject(project)) {
      announce(project.createError || unavailable, true);
      return;
    }
    const body: DraftInput = {};
    if (projectId !== null) {
      body.projectId = projectId;
      if (projectId.startsWith("target:") || targetProject(project) || operationId)
        body.operationId = operationId || nativeOperation(projectId);
      else {
        const legacyRequest = requestId || draftRequests.get(projectId);
        if (legacyRequest) body.requestId = legacyRequest;
      }
    }
    if (consentToken) body.consent = consentToken;
    ports.setCreating(true);
    button.disabled = true;
    ports.invalidateSnapshot();
    render();
    try {
      const result = await ports.request("/api/new", body) as NewSessionResult;
      if (result.consent) {
        const { token, summary, changes } = result.consent;
        consent = { projectId, operationId: body.operationId, requestId: body.requestId, token };
        elements.consentTarget.textContent = summary;
        elements.consentChanges.replaceChildren(...changes.map(change => {
          const item = document.createElement("li");
          item.textContent = change;
          return item;
        }));
        consentDialog.showModal();
        return;
      }
      if (projectId !== null && result.pending) {
        const draft: DraftRequest = body.operationId ? { projectId, operationId: body.operationId } :
          { projectId, requestId: result.requestId || body.requestId };
        if (draft.requestId) draftRequests.set(projectId, draft.requestId);
        recover(draft, result.error ?? "");
        return;
      }
      if (result.registration) {
        announce("请在 Codex 确认目标目录。登记后若未进入输入页，回到看板重新打开同一项目。");
        return;
      }
      if (projectId !== null) {
        if (body.operationId) clearNativeOperation(projectId);
        draftRequests.delete(projectId);
      }
      const remote = body.operationId || body.requestId || project?.remote;
      announce(remote
        ? "已请求打开远程原生输入页。请核对主机和目录，发送第一句话后才会创建会话；回到看板刷新可确认项目归属。"
        : "已发送新建会话页面打开请求。");
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      if (projectId !== null && body.operationId)
        recover({ projectId, operationId: body.operationId }, "新建尚未完成，请核对客户端和目标后重试。" + message);
      announce(message, true);
    } finally {
      ports.setCreating(false);
      button.disabled = false;
      render();
      await load();
    }
  }

  function loadAfterClose() {
    updateFilterLock();
    void load();
  }

  elements.consentForm.onsubmit = event => {
    event.preventDefault();
    if (!consentDialog.open || !consent) return;
    const accepted = consent;
    consentDialog.close();
    void create(accepted.projectId, accepted.operationId, accepted.token, accepted.requestId);
  };
  elements.consentCancel.onclick = () => consentDialog.close();
  consentDialog.addEventListener("close", () => {
    if (consentDialog.open) return;
    consent = undefined;
    loadAfterClose();
  });
  elements.recoveryLater.onclick = () => recoveryDialog.close();
  recoveryDialog.addEventListener("close", loadAfterClose);
  elements.recoveryRetry.onclick = () => {
    if (!recovering || !recoveryDialog.open) return;
    recoveryDialog.close();
    const { projectId, operationId, requestId } = recovering;
    void create(projectId, operationId, undefined, requestId);
  };
  recoveryRestart.onclick = async () => {
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
  };

  function update(snapshot: BoardSnapshot) {
    for (const draft of snapshot.pendingDrafts ?? []) draftRequests.set(draft.project, draft.requestId);
  }

  return {
    create, canCreateProject, update,
    get openDialog() { return consentDialog.open || recoveryDialog.open; },
    hasPendingDraft: (projectId: string) => draftRequests.has(projectId),
  };
}
