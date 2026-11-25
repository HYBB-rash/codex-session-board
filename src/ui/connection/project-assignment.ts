import type { CatalogProject } from "../browse/scope";
import type { BoardSession } from "../session";

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
    request(
        path: "/api/project/assign",
        body: { id: string; projectId: string; confirmed: true },
    ): Promise<unknown>;
    announce(message: string): void;
    load(): Promise<void>;
}

export function mountProjectAssignment(elements: AssignmentElements, ports: AssignmentPorts) {
    let selectedSession: BoardSession | undefined;

    function projectsFor(session: BoardSession) {
        return ports
            .projects()
            .filter(
                (item) =>
                    item.origin === "targetServer" &&
                    item.hostId === session.hostId &&
                    !item.pendingDeletion &&
                    item.paths?.length === 1 &&
                    item.paths[0] === session.cwd,
            );
    }

    function canAssign(session: BoardSession) {
        return !!(
            session.remote &&
            session.catalogCurrent &&
            session.ordinary &&
            !session.archived &&
            !session.readOnly &&
            session.canOpen !== false &&
            !session.projectId &&
            projectsFor(session).length
        );
    }

    function open(session: BoardSession) {
        const { dialog, thread, project, error } = elements;
        if (ports.locked() || !canAssign(session)) return;
        selectedSession = session;
        thread.textContent = `${session.title}\n${session.threadId}\n${session.cwd}`;
        project.replaceChildren(
            ...projectsFor(session).map((item) => new Option(ports.projectLabel(item), item.id)),
        );
        error.textContent = "";
        dialog.showModal();
        ports.updateFilterLock();
    }

    async function submit(event: SubmitEvent) {
        const selected = selectedSession;
        const { dialog, project, error, cancel, save } = elements;
        event.preventDefault();
        if (!selected || !dialog.open || ports.saving() || ports.creating()) return;
        if (!projectsFor(selected).some((item) => item.id === project.value)) {
            error.textContent = "项目暂不可用，请刷新后重新选择。";
            return;
        }
        dialog.setAttribute("closedby", "none");
        ports.setSaving(true);
        ports.updateFilterLock();
        ports.invalidateSnapshot();
        for (const control of [save, cancel, project]) control.disabled = true;
        try {
            await ports.request("/api/project/assign", {
                id: selected.id,
                projectId: project.value,
                confirmed: true,
            });
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
    }

    const { cancel, dialog, form } = elements;
    cancel.onclick = () => dialog.close();
    dialog.addEventListener("cancel", (event) => {
        if (ports.saving()) event.preventDefault();
    });
    dialog.addEventListener(
        "keydown",
        (event) => {
            if (ports.saving() && event.key === "Escape") event.preventDefault();
        },
        true,
    );
    dialog.addEventListener("close", () => {
        if (dialog.open) return;
        selectedSession = undefined;
        ports.updateFilterLock();
        void ports.load();
    });
    form.onsubmit = (event) => submit(event);

    return {
        canAssign,
        open,
        get openDialog() {
            return elements.dialog.open;
        },
    };
}
