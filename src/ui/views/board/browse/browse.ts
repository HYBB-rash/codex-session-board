import type { BoardSnapshot } from "../../../board-start";
import type { BoardSession, ProjectTag } from "../../../board-view";
import { mountBoardFilters } from "./filters";

export interface HostIdentity {
    hostId?: string;
    hostLabel?: string;
    enabled?: boolean;
}

export interface HostOption extends HostIdentity {
    value: string;
}

export interface HostCatalog {
    error?: string | null;
    cached?: boolean;
    complete?: boolean;
    pending?: boolean;
    capturedAt?: number | null;
    issues?: unknown[];
    coverage?: unknown;
}

export interface CatalogProject extends HostIdentity {
    id: string;
    label: string;
    paths: string[];
    remote?: boolean;
    cached?: boolean;
    pendingDeletion?: boolean;
    canCreate?: boolean;
    createError?: string;
    origin?: "targetServer" | "targetLegacy" | "clientRemote";
}

interface BrowsePorts {
    snapshot(): BoardSnapshot;
    locked(): boolean;
    changed(): void;
    announce(message: string): void;
}

const projectOrigins = {
    targetServer: "主机项目",
    targetLegacy: "主机旧项目",
    clientRemote: "本机登记",
};
const hostValue = (hostId?: string) => `host:${hostId}`;
const hostLabel = (host: HostIdentity) => host.hostLabel || host.hostId;

function optionsForHosts({ health, sessions, projectCatalog }: BoardSnapshot) {
    const hosts = new Map<string, HostOption>();
    for (const host of [
        ...(health.remoteConnections ?? []),
        ...(health.remoteConnectionErrors ?? []),
        ...sessions.filter((row) => row.remote),
        ...projectCatalog.items.filter((item) => item.remote),
        ...(projectCatalog.remoteHosts ?? []),
        ...(health.remoteRegistry ?? []),
    ]) {
        if (host.hostId) {
            const value = hostValue(host.hostId);
            hosts.set(value, { ...host, value });
        }
    }
    return [{ value: "local", hostLabel: "本机" }, ...hosts.values()];
}

function projectsOnHost({ projectCatalog }: BoardSnapshot, host: string) {
    return projectCatalog.items.filter((item) =>
        host === "local" ? !item.remote : item.remote && hostValue(item.hostId) === host,
    );
}

function nameForHost(host: HostOption, hosts: HostOption[]) {
    const label = hostLabel(host);
    const duplicate = hosts.filter((other) => hostLabel(other) === label).length > 1;
    return `${label}${duplicate ? ` · ${host.hostId || "local"}` : ""}`;
}

function sessionsOnHost({ sessions }: BoardSnapshot, host: string) {
    return sessions.filter(
        (session) =>
            host === "all" ||
            (host === "local"
                ? !session.remote
                : session.remote && hostValue(session.hostId) === host),
    );
}

function labelForProject(item: CatalogProject, snapshot: BoardSnapshot) {
    const name = (entry: CatalogProject) => {
        const suffix = ` · ${entry.hostLabel}`;
        return entry.remote && entry.label.endsWith(suffix)
            ? entry.label.slice(0, -suffix.length)
            : entry.label;
    };
    const itemName = name(item);
    const duplicates = projectsOnHost(
        snapshot,
        item.remote ? hostValue(item.hostId) : "local",
    ).filter((candidate) => name(candidate) === itemName);
    const origin = item.origin && projectOrigins[item.origin];
    const label = itemName + (origin ? ` · ${origin}` : "");
    if (duplicates.length < 2) return label;
    const paths = item.paths.join(" · ");
    const identical = duplicates.filter(
        (candidate) => candidate.origin === item.origin && candidate.paths.join(" · ") === paths,
    );
    return `${label} · ${paths}${identical.length > 1 ? ` · ${item.id}` : ""}`;
}

function presentProject(session: BoardSession, snapshot: BoardSnapshot, host: string): ProjectTag {
    const catalog = snapshot.projectCatalog;
    const item = catalog.items.find((candidate) => candidate.id === session.projectId);
    const pending = Boolean(
        session.projectIssue ||
            (session.projectId && !item) ||
            (!session.remote && !catalog.loaded),
    );
    const cached = Boolean(item && (item.cached || (!session.remote && catalog.error)));
    const label = item
        ? labelForProject(item, snapshot)
        : pending
          ? "项目待确认"
          : `默认工作区${session.remote ? ` · ${hostLabel(session)}` : ""}`;
    const hostSuffix = session.remote && host === "all" ? ` · ${hostLabel(session)}` : "";
    const summaryLabel = hostSuffix && !label.includes(hostSuffix) ? label + hostSuffix : label;
    const info = [label];
    if (item) info.push(`项目目录：${item.paths.join(" · ")}`);
    info.push(`工作目录：${session.cwd || "未记录"}`);
    const source =
        session.source === "runtime"
            ? "自动流转"
            : session.source === "codex"
              ? "原生状态"
              : "历史状态";
    info.push(`状态来源：${session.manual ? "人工确认" : source}`);
    if (session.remote) info.push(`主机：${hostLabel(session)}`);
    if (pending || cached)
        info.push(session.projectIssue || "项目资料尚未确认更新，当前显示上次可用信息。");
    return {
        label,
        summary: summaryLabel + (pending && item ? " · 归属待确认" : cached ? " · 缓存" : ""),
        pending: pending || cached,
        info,
    };
}

export function mountBoardBrowse(container: HTMLElement, scope: HTMLElement, ports: BrowsePorts) {
    const hostOptions = () => optionsForHosts(ports.snapshot());
    const hostProjects = (host = selectedHost()) => projectsOnHost(ports.snapshot(), host);
    const hostName = (host: HostOption) => nameForHost(host, hostOptions());
    const hostSessions = (host = selectedHost()) => sessionsOnHost(ports.snapshot(), host);
    const hostCatalog = (host: string) =>
        ports.snapshot().health.remoteConnections?.find((item) => hostValue(item.hostId) === host)
            ?.catalog;
    const projectLabel = (item: CatalogProject) => labelForProject(item, ports.snapshot());
    const projectPresentation = (session: BoardSession) =>
        presentProject(session, ports.snapshot(), selectedHost());
    const filters = mountBoardFilters(container, scope, {
        hosts: hostOptions,
        hostName,
        hostProjects,
        hostSessions,
        hostCatalog,
        projectLabel,
        locked: ports.locked,
        announce: ports.announce,
        changed() {
            const root = container.getRootNode() as ShadowRoot | Document;
            for (const cards of root.querySelectorAll<HTMLElement>(".cards")) cards.scrollTop = 0;
            ports.changed();
        },
    });
    function selectedHost() {
        return filters.state.host;
    }
    return {
        filters,
        hostOptions,
        hostProjects,
        hostSessions,
        hostName,
        projectLabel,
        projectPresentation,
        visibleSessions: () => filters.view.visible,
    };
}
