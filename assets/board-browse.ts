import { mountBoardFilters } from "./board-filters";
import type { BoardSnapshot } from "./board-start";
import type { BoardSession, ProjectTag } from "./board-view";

export interface HostIdentity {
  hostId?: string;
  hostLabel?: string;
  enabled?: boolean;
}

export interface HostOption extends HostIdentity {
  value: string;
}

export interface HostCatalog {
  error?: string;
  cached?: boolean;
  complete?: boolean;
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

const projectOrigins = { targetServer: "主机项目", targetLegacy: "主机旧项目", clientRemote: "本机登记" };
const hostValue = (hostId?: string) => `host:${hostId}`;
const hostLabel = (host: HostIdentity) => host.hostLabel || host.hostId;

export function mountBoardBrowse(container: HTMLElement, scope: HTMLElement, ports: BrowsePorts) {
  function hostOptions() {
    const { health, sessions, projectCatalog } = ports.snapshot();
    const hosts = new Map<string, HostOption>();
    for (const host of [...health.remoteConnections ?? [], ...health.remoteConnectionErrors ?? [],
      ...sessions.filter(row => row.remote), ...projectCatalog.items.filter(item => item.remote),
      ...projectCatalog.remoteHosts ?? [], ...health.remoteRegistry ?? []]) {
      if (host.hostId) {
        const value = hostValue(host.hostId);
        hosts.set(value, { ...host, value });
      }
    }
    return [{ value: "local", hostLabel: "本机" }, ...hosts.values()];
  }

  function hostProjects(host = selectedHost()) {
    return ports.snapshot().projectCatalog.items.filter(item =>
      host === "local" ? !item.remote : item.remote && hostValue(item.hostId) === host);
  }

  function hostName(host: HostOption) {
    const label = hostLabel(host);
    const duplicate = hostOptions().filter(other => hostLabel(other) === label).length > 1;
    return `${label}${duplicate ? " · " + (host.hostId || "local") : ""}`;
  }

  function hostSessions(host = selectedHost()) {
    return ports.snapshot().sessions.filter(session => host === "all" ||
      (host === "local" ? !session.remote : session.remote && hostValue(session.hostId) === host));
  }

  function hostCatalog(host: string) {
    return ports.snapshot().health.remoteConnections?.find(item => hostValue(item.hostId) === host)?.catalog;
  }

  function projectLabel(item: CatalogProject) {
    const name = (entry: CatalogProject) => {
      const suffix = " · " + entry.hostLabel;
      return entry.remote && entry.label.endsWith(suffix) ? entry.label.slice(0, -suffix.length) : entry.label;
    };
    const itemName = name(item);
    const duplicates = hostProjects(item.remote ? hostValue(item.hostId) : "local")
      .filter(candidate => name(candidate) === itemName);
    const origin = item.origin && projectOrigins[item.origin];
    const label = itemName + (origin ? " · " + origin : "");
    if (duplicates.length < 2) return label;
    const paths = item.paths.join(" · ");
    const identical = duplicates.filter(candidate => candidate.origin === item.origin && candidate.paths.join(" · ") === paths);
    return label + " · " + paths + (identical.length > 1 ? " · " + item.id : "");
  }

  function projectPresentation(session: BoardSession): ProjectTag {
    const catalog = ports.snapshot().projectCatalog;
    const item = catalog.items.find(candidate => candidate.id === session.projectId);
    const pending = Boolean(session.projectIssue || (session.projectId && !item) || (!session.remote && !catalog.loaded));
    const cached = Boolean(item && (item.cached || !session.remote && catalog.error));
    const label = item ? projectLabel(item) : pending ? "项目待确认" :
      "默认工作区" + (session.remote ? " · " + hostLabel(session) : "");
    const hostSuffix = session.remote && selectedHost() === "all" ? " · " + hostLabel(session) : "";
    const summaryLabel = hostSuffix && !label.includes(hostSuffix) ? label + hostSuffix : label;
    const info = [label];
    if (item) info.push("项目目录：" + item.paths.join(" · "));
    info.push("工作目录：" + (session.cwd || "未记录"));
    const source = session.source === "runtime" ? "自动流转" : session.source === "codex" ? "原生状态" : "历史状态";
    info.push("状态来源：" + (session.manual ? "人工确认" : source));
    if (session.remote) info.push("主机：" + hostLabel(session));
    if (pending || cached) info.push(session.projectIssue || "项目资料尚未确认更新，当前显示上次可用信息。");
    return { label, summary: summaryLabel + (pending && item ? " · 归属待确认" : cached ? " · 缓存" : ""),
      pending: pending || cached, info };
  }

  const filters = mountBoardFilters(container, scope, {
    hosts: hostOptions, hostName, hostProjects, hostSessions, hostCatalog, projectLabel,
    locked: ports.locked, announce: ports.announce,
    changed() {
      for (const cards of document.querySelectorAll<HTMLElement>(".cards")) cards.scrollTop = 0;
      ports.changed();
    },
  });
  function selectedHost() {
    return filters.state.host;
  }
  return { filters, hostOptions, hostProjects, hostSessions, hostName, projectLabel, projectPresentation,
    visibleSessions: () => filters.view.visible };
}
