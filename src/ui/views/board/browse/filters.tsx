import { computed, createApp, nextTick, shallowReactive, Teleport } from "vue";
import type { BoardSnapshot } from "../controller";
import type { BoardSession } from "../session";
import type { CatalogProject, HostCatalog, HostOption } from "./scope";

interface FilterOption {
    value: string;
    label: string;
    path?: string;
    hostId?: string | undefined;
}

interface Sources {
    hosts(): HostOption[];
    hostName(host: HostOption): string;
    hostProjects(host: string): CatalogProject[];
    hostSessions(host: string): BoardSession[];
    hostCatalog(host: string): HostCatalog | undefined;
    projectLabel(project: CatalogProject): string;
    locked(): boolean;
    changed(): void;
    announce(text: string): void;
}

const histories = [
    { value: "active", label: "活动" },
    { value: "archived", label: "归档" },
    { value: "all", label: "全部" },
    { value: "cached", label: "历史缓存" },
];

class BoardFilters {
    readonly state = shallowReactive({
        host: "local",
        project: "all",
        history: "active",
        search: false,
        query: "",
    });
    readonly view = shallowReactive({
        hosts: [] as FilterOption[],
        projects: [] as FilterOption[],
        hostLabel: "本机",
        scope: "",
        visible: [] as BoardSession[],
        total: 0,
        locked: false,
        projectUnavailable: true,
    });
    private searchInput: HTMLInputElement | null = null;
    private projectMenu: HTMLSelectElement | null = null;
    private catalogKey = "";
    private catalogOptions: FilterOption[] = [];

    constructor(
        private readonly sources: Sources,
        private readonly scope: HTMLElement,
    ) {}

    private projectOptions(host: string) {
        const items = this.sources.hostProjects(host);
        const key = JSON.stringify([host, items]);
        if (key === this.catalogKey) return this.catalogOptions;
        const all = host === "all";
        const options: FilterOption[] = [
            { value: "all", label: all ? "先选择一台主机" : "全部项目" },
        ];
        if (!all) options.push({ value: "default", label: "默认工作区" });
        for (const item of items) {
            options.push({
                value: `project:${item.id}`,
                label: this.sources.projectLabel(item),
                path: item.paths.join(" · "),
                hostId: item.remote ? item.hostId : undefined,
            });
        }
        this.catalogKey = key;
        this.catalogOptions = options;
        return options;
    }

    private updateHosts() {
        const { state, view, sources } = this;
        const hosts: FilterOption[] = [
            { value: "all", label: "全部主机" },
            ...sources.hosts().map((host) => ({
                value: host.value,
                label: sources.hostName(host) + (host.enabled === false ? " · 未连接" : ""),
            })),
        ];
        const currentHost = hosts.find((option) => option.value === state.host);
        view.hostLabel =
            currentHost?.label ?? `${view.hostLabel.replace(/ · 已移除$/, "")} · 已移除`;
        if (!currentHost) hosts.push({ value: state.host, label: view.hostLabel });
        view.hosts = hosts;
    }

    private updateProjects({ projectCatalog, health }: BoardSnapshot) {
        const { state, view, sources } = this;
        const allHosts = state.host === "all",
            local = state.host === "local";
        const remote = sources.hostCatalog(state.host);
        view.projectUnavailable = allHosts || (!projectCatalog.loaded && (local || !remote));
        let projects = this.projectOptions(state.host);
        let project = projects.find((option) => option.value === state.project);
        if (!project) {
            const failed = local
                ? projectCatalog.error || !projectCatalog.loaded
                : remote
                  ? remote.error || remote.cached || !remote.complete
                  : health.remoteError;
            if (failed) {
                const previous =
                    view.projects.find((option) => option.value === state.project)?.label ||
                    "上次选择的项目";
                project = {
                    value: state.project,
                    label: `${previous.replace(/ · 资料暂不可读$/, "")} · 资料暂不可读`,
                };
                projects = [...projects, project];
            } else {
                project = projects[0];
                state.project = "all";
                sources.announce("项目已移除，已回到当前主机的全部项目。");
            }
        }
        view.projects = projects;
        const history = histories.find((option) => option.value === state.history)?.label ?? "活动";
        const projectScope = state.search
            ? "全部项目 · 搜索"
            : allHosts
              ? "全部会话"
              : (project?.label ?? "全部项目");
        view.scope = `${view.hostLabel} · ${projectScope} · ${history}`;
    }

    private updateSessions({ projectCatalog }: BoardSnapshot) {
        const { state, view, sources } = this;
        const sessions = sources.hostSessions(state.host).filter((session) => {
            if (state.history === "cached") return session.presence === "cachedUnknown";
            if (session.presence === "cachedUnknown" && !session.catalogCurrent) return false;
            return (
                state.history === "all" ||
                Boolean(session.archived) === (state.history === "archived")
            );
        });
        view.total = sessions.length;
        const query = state.query.trim().toLowerCase();
        view.visible = sessions.filter((session) => {
            if (state.search) {
                if (!query) return true;
                const item = projectCatalog.items.find(
                    (project) => project.id === session.projectId,
                );
                return [
                    session.title,
                    session.cwd,
                    session.hostLabel,
                    item && sources.projectLabel(item),
                ]
                    .filter(Boolean)
                    .join(" ")
                    .toLowerCase()
                    .includes(query);
            }
            if (state.project === "all") return true;
            if (state.project === "default")
                return session.projectId === null && !session.projectIssue;
            return `project:${session.projectId}` === state.project;
        });
    }

    readonly update = (snapshot: BoardSnapshot) => {
        this.updateHosts();
        this.updateProjects(snapshot);
        this.updateSessions(snapshot);
        this.updateLock();
    };

    readonly updateLock = () => {
        this.view.locked = this.sources.locked();
    };

    private select(field: "host" | "project" | "history", event: Event) {
        const { state, view, sources } = this;
        const menu = event.currentTarget as HTMLSelectElement;
        if (sources.locked() || (field === "project" && view.projectUnavailable)) {
            menu.value = state[field];
            return;
        }
        state[field] = menu.value;
        if (field === "host") {
            state.project = "all";
            if (state.host === "local") state.history = "active";
            sources.announce("");
        }
        sources.changed();
    }
    private readonly toggle = async () => {
        const { state, sources } = this;
        if (sources.locked()) return;
        state.search = !state.search;
        sources.changed();
        await nextTick();
        (state.search ? this.searchInput : this.projectMenu)?.focus();
    };

    private options(items: FilterOption[]) {
        return items.map((item) => (
            <option
                key={item.value}
                value={item.value}
                title={item.path}
                data-host-id={item.hostId}
            >
                {item.label}
            </option>
        ));
    }

    private readonly projectNodes = computed(() => this.options(this.view.projects));

    private renderMenus() {
        const { state, view } = this;
        return (
            <>
                <label class="filter-field">
                    主机
                    <select
                        id="host-filter"
                        aria-label="选择主机"
                        value={state.host}
                        disabled={view.locked}
                        onChange={(event) => this.select("host", event)}
                    >
                        {this.options(view.hosts)}
                    </select>
                </label>
                <div class="filter-field">
                    <span>项目</span>
                    <select
                        id="project-filter"
                        aria-label="选择项目"
                        ref={(node) => {
                            this.projectMenu = node as HTMLSelectElement | null;
                        }}
                        hidden={state.search}
                        value={state.project}
                        disabled={view.locked || view.projectUnavailable}
                        onChange={(event) => this.select("project", event)}
                    >
                        {this.projectNodes.value}
                    </select>
                </div>
                <label class="filter-field">
                    会话
                    <select
                        id="history-filter"
                        aria-label="选择会话范围"
                        value={state.history}
                        disabled={view.locked}
                        onChange={(event) => this.select("history", event)}
                    >
                        {this.options(histories)}
                    </select>
                </label>
            </>
        );
    }

    readonly render = () => {
        const { state, view, sources, scope } = this;
        return (
            <>
                {this.renderMenus()}
                <input
                    id="search"
                    type="search"
                    placeholder={`搜索${view.hostLabel}上的会话…`}
                    aria-label="搜索会话、项目名称或路径"
                    ref={(node) => {
                        this.searchInput = node as HTMLInputElement | null;
                    }}
                    value={state.query}
                    hidden={!state.search}
                    disabled={view.locked}
                    onInput={(event) => {
                        state.query = (event.currentTarget as HTMLInputElement).value;
                        sources.changed();
                    }}
                />
                <button
                    id="filter-toggle"
                    class="quiet"
                    type="button"
                    aria-controls="project-filter search"
                    aria-pressed={state.search}
                    disabled={view.locked}
                    onClick={this.toggle}
                >
                    {state.search ? "选择项目" : "搜索会话"}
                </button>
                <span
                    id="count"
                    class="count"
                >{`显示 ${view.visible.length} / 当前主机范围共 ${view.total} 条`}</span>
                <Teleport to={scope}>{view.scope}</Teleport>
            </>
        );
    };
}

export function mountBoardFilters(container: HTMLElement, scope: HTMLElement, sources: Sources) {
    const filters = new BoardFilters(sources, scope);
    const app = createApp({ setup: () => filters.render });
    app.mount(container);
    return {
        state: filters.state,
        view: filters.view,
        update: filters.update,
        unmount: () => app.unmount(),
        updateLock: filters.updateLock,
    };
}
