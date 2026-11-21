import type { CatalogProject } from "./board-browse";
import type { BoardSession } from "./board-view";

interface AssignmentElements {
  dialog: HTMLDialogElement;
  form: HTMLFormElement;
  thread: HTMLElement;
  project: HTMLSelectElement;
  error: HTMLElement;
  cancel: HTMLButtonElement;
  save: HTMLButtonElement;
}

interface AssignmentPorts {
  projects(): CatalogProject[];
  projectLabel(project: CatalogProject): string;
  locked(): boolean;
  saving(): boolean;
  creating(): boolean;
  setSaving(value: boolean): void;
  updateFilterLock(): void;
  invalidateSnapshot(): void;
  request(path: "/api/project/assign", body: { id: string; projectId: string; confirmed: true }): Promise<unknown>;
  announce(message: string): void;
  load(): Promise<void>;
}

export function mountProjectAssignment(elements: AssignmentElements, ports: AssignmentPorts) {
  const { dialog, form, thread, project, error, cancel, save } = elements;
  let selected: BoardSession | undefined;

  function projectsFor(session: BoardSession) {
    return ports.projects().filter(item => item.origin === "targetServer" && item.hostId === session.hostId &&
      !item.pendingDeletion && item.paths?.length === 1 && item.paths[0] === session.cwd);
  }

  function canAssign(session: BoardSession) {
    return !!(session.remote && session.catalogCurrent && session.ordinary && !session.archived &&
      session.canOpen !== false && !session.projectId && projectsFor(session).length);
  }

  function open(session: BoardSession) {
    if (ports.locked() || !canAssign(session)) return;
    selected = session;
    dialog.dataset.id = session.id;
    thread.textContent = `${session.title}\n${session.threadId}\n${session.cwd}`;
    project.replaceChildren(...projectsFor(session).map(item => new Option(ports.projectLabel(item), item.id)));
    error.textContent = "";
    dialog.showModal();
    ports.updateFilterLock();
  }

  cancel.onclick = () => dialog.close();
  dialog.addEventListener("cancel", event => {
    if (ports.saving()) event.preventDefault();
  });
  dialog.addEventListener("keydown", event => {
    if (ports.saving() && event.key === "Escape") event.preventDefault();
  }, true);
  dialog.addEventListener("close", () => {
    if (dialog.open) return;
    selected = undefined;
    ports.updateFilterLock();
    void ports.load();
  });

  form.onsubmit = async event => {
    event.preventDefault();
    if (!selected || !dialog.open || ports.saving() || ports.creating()) return;
    if (!projectsFor(selected).some(item => item.id === project.value)) {
      error.textContent = "项目暂不可用，请刷新后重新选择。";
      return;
    }
    dialog.setAttribute("closedby", "none");
    ports.setSaving(true);
    ports.updateFilterLock();
    ports.invalidateSnapshot();
    for (const control of [save, cancel, project]) control.disabled = true;
    try {
      await ports.request("/api/project/assign", { id: selected.id, projectId: project.value, confirmed: true });
      dialog.close();
      ports.announce("已读回项目归属。可刷新目录查看归类结果。");
    } catch (cause) {
      error.textContent = cause instanceof Error ? cause.message : String(cause);
    } finally {
      ports.setSaving(false);
      dialog.removeAttribute("closedby");
      for (const control of [save, cancel, project]) control.disabled = false;
      ports.updateFilterLock();
      await ports.load();
    }
  };

  return { open, canAssign, get openDialog() { return dialog.open; } };
}
