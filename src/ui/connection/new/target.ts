import type { CatalogProject, HostOption } from "../../browse/scope";
import type { BoardSnapshot } from "../../controller";

interface TargetElements {
    button: HTMLButtonElement;
    dialog: HTMLDialogElement;
    form: HTMLFormElement;
    host: HTMLSelectElement;
    project: HTMLSelectElement;
    submit: HTMLButtonElement;
    hint: HTMLElement;
    cancel: HTMLButtonElement;
}

interface TargetPorts {
    snapshot(): BoardSnapshot;
    scope(): { host: string; project: string; search: boolean };
    hostOptions(): HostOption[];
    hostProjects(host?: string): CatalogProject[];
    hostName(host: HostOption): string;
    projectLabel(project: CatalogProject): string;
    locked(): boolean;
    saving(): boolean;
    creating(): boolean;
    canCreateProject(project?: CatalogProject): boolean;
    create(project: string | null): Promise<void>;
    updateFilterLock(): void;
    announce(message: string, error?: boolean): void;
    load(): Promise<void>;
}

const removedHost = "这台主机已移除，不能在这里新建会话。";

export function mountBoardNewTarget(elements: TargetElements, ports: TargetPorts) {
    function optionsForHost() {
        const { host, project, submit, hint } = elements;
        const { hostProjects, projectLabel } = ports;
        const projects = hostProjects(host.value);
        const options = projects
            .filter(ports.canCreateProject)
            .map((item) => new Option(projectLabel(item), item.id));
        if (host.value === "local") options.unshift(new Option("本机新会话输入页", ""));
        project.replaceChildren(...options);
        project.required = host.value !== "local";
        submit.disabled = !options.length;
        const reasons = new Set(projects.map((item) => item.createError || ""));
        const reason = reasons.size === 1 ? [...reasons][0] : "";
        hint.textContent = options.length
            ? "打开原生输入页后核对主机和目录。由你发送第一句话，才会创建会话。"
            : reason || "此主机没有支持新建会话的项目。已有项目与会话仍可查看。";
    }

    function openSelectedProject(value: string) {
        const project = ports
            .hostProjects()
            .find((candidate) => `project:${candidate.id}` === value);
        if (!project) ports.announce("项目暂不可用，请刷新后重新选择。", true);
        else void ports.create(project.id);
    }

    function open() {
        const { dialog, host } = elements;
        const { announce, create } = ports;
        if (ports.saving() || ports.locked()) return;
        const scope = ports.scope();
        const registry = ports.snapshot().health.remoteRegistry ?? [];
        const registered = (value: string) =>
            registry.some((item) => `host:${item.hostId}` === value);
        if (scope.host.startsWith("host:") && !registered(scope.host)) {
            announce(removedHost, true);
            return;
        }
        if (!scope.search && scope.project.startsWith("project:")) {
            openSelectedProject(scope.project);
            return;
        }
        if (scope.host === "local") {
            void create(null);
            return;
        }
        host.replaceChildren(
            ...ports
                .hostOptions()
                .filter(
                    (item) =>
                        (item.value === "local" || registered(item.value)) &&
                        (scope.host === "all" || item.value === scope.host),
                )
                .map((item) => new Option(ports.hostName(item), item.value)),
        );
        host.disabled = scope.host !== "all";
        if (!host.options.length) {
            announce(removedHost, true);
            return;
        }
        optionsForHost();
        dialog.showModal();
        ports.updateFilterLock();
    }

    function submit(event: SubmitEvent) {
        const { dialog, project, submit, host } = elements;
        const { announce, create, hostProjects } = ports;
        event.preventDefault();
        if (!dialog.open || submit.disabled || !project.reportValidity()) return;
        const selected = project.value;
        if (!selected && host.value !== "local") return;
        if (selected && !hostProjects(host.value).some((item) => item.id === selected)) {
            announce("项目暂不可用，请刷新后重新选择。", true);
            return;
        }
        dialog.close();
        void create(selected || null);
    }

    function update() {
        const { button } = elements;
        button.disabled = ports.creating() || ports.saving();
        button.title = "打开新建会话页，不发送输入";
    }

    const { button, dialog, form, host, cancel } = elements;
    button.onclick = open;
    host.onchange = optionsForHost;
    dialog.addEventListener("close", () => {
        ports.updateFilterLock();
        void ports.load();
    });
    cancel.onclick = () => dialog.close();
    form.onsubmit = submit;

    return {
        get openDialog() {
            return elements.dialog.open;
        },
        update,
    };
}
