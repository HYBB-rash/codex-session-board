import type { AutoArchiveHealth } from "./board-archive";
import type { CatalogProject, HostCatalog, HostIdentity, HostOption } from "./board-browse";
import type { BoardSnapshot } from "./board-start";
import type { BoardSession } from "./board-view";

interface HostHealth extends AutoArchiveHealth {
  error?: string | null;
  runtimeError?: string | null;
  nativeError?: string | null;
  nativeStageError?: string | null;
  writeError?: string | null;
  archiveReadError?: string | null;
  historyError?: string | null;
  lastSuccessAt?: number | null;
  lastScanAt?: number | null;
  runtimeLastAt?: number | null;
  scanComplete?: boolean;
  refreshing?: boolean;
  runtimeVerified?: boolean;
  nativeSections?: boolean;
  nativeVerified?: boolean;
}

export interface BoardHealth extends HostHealth {
  sshAliasSetup?: boolean;
  nativeConnections?: boolean;
  remoteError?: string | null;
  remoteCacheErrors?: unknown[];
  remoteConnections?: (HostIdentity & HostHealth & { connected?: boolean; catalog?: HostCatalog })[];
  remoteConnectionErrors?: (HostIdentity & { error?: string })[];
  remoteHosts?: (HostIdentity & { connectionRequired?: boolean; error?: string })[];
  remoteRegistry?: HostIdentity[];
}

type Diagnostic = [label: string, error: string];
type PossibleDiagnostic = [label: string, error: string | false | null | undefined];
function diagnosticsFrom(entries: PossibleDiagnostic[]) {
  return entries.filter((entry): entry is Diagnostic => Boolean(entry[1]));
}

export interface HealthReport {
  key: string;
  label: string;
  status: string;
  ready?: boolean;
  attention: boolean;
  effects: string[];
  diagnostics: Diagnostic[];
  lastRead?: number | null;
  canConnect?: boolean;
  hostId?: string;
}

interface HealthSources {
  snapshot(): BoardSnapshot;
  selectedHost(): string;
  selectedHostLabel(): string;
  hostOptions(): HostOption[];
  hostSessions(host: string): BoardSession[];
  hostProjects(host: string): CatalogProject[];
  hostName(host: HostOption): string;
  readError(): string | null;
  date(value?: number | null): string;
}

export function createBoardHealthReports(sources: HealthSources) {
  const { snapshot, hostOptions, hostSessions, hostProjects, hostName, date } = sources;

  function hostHealth(host: HostOption): HealthReport {
    const { health, projectCatalog, legacyMode } = snapshot();
    const local = host.value === "local", rows = hostSessions(host.value);
    const remote = health.remoteConnections?.find(item => item.hostId === host.hostId);
    const current: HostHealth = local ? health : remote ?? {};
    const scan = health.remoteHosts?.find(item => item.hostId === host.hostId);
    const connection = health.remoteConnectionErrors?.find(item => item.hostId === host.hostId);
    const idle = !local && (host.enabled === false || !legacyMode && !health.remoteRegistry?.some(item => item.hostId === host.hostId));
    const catalog = local ? undefined : remote?.catalog;
    const projectError = local ? projectCatalog.error : catalog?.error;
    const listError = current.error || connection?.error || (!scan?.connectionRequired && scan?.error);
    const activeRows = rows.filter(row => !row.archived && row.presence !== "cachedUnknown");
    const readonly = activeRows.filter(row => row.readOnly).length;
    const blocked = Boolean(current.writeError || activeRows.length && readonly === activeRows.length);
    const partial = Boolean(current.nativeStageError || current.runtimeVerified && current.runtimeError);
    const catalogIssues = catalog?.issues?.length
      ? catalog.issues.map(issue => typeof issue === "string" ? issue : JSON.stringify(issue)).join("\n") : null;
    const diagnostics = diagnosticsFrom([["会话列表", listError], ["自动状态", current.runtimeError],
      ["状态分组", current.nativeError], ["状态更新", current.nativeStageError], ["修改权限", current.writeError],
      ["项目列表", projectError], ["目录归属", catalogIssues], ["轮次历史", current.historyError], ["归档历史", current.archiveReadError],
      ["定时归档", current.autoArchiveError]]);
    if (catalog?.coverage) diagnostics.push(["目录覆盖", JSON.stringify(catalog.coverage)]);
    const incomplete = current.lastScanAt && !current.scanComplete && !current.refreshing;
    const faults = diagnostics.some(([label]) => label !== "目录覆盖" &&
      (label !== "自动状态" || current.lastScanAt || current.lastSuccessAt || current.runtimeLastAt));
    const pendingProject = rows.some(row => row.projectIssue);
    const attention = !idle && Boolean(faults || blocked || readonly || incomplete || catalog && !catalog.complete || pendingProject);
    const cachedProjects = hostProjects(host.value).some(item => item.cached);
    let status = !current.lastSuccessAt ? rows.length ? "显示已有会话，更新时间待确认" : "正在等待读取会话" :
      !current.scanComplete ? "会话列表尚未读取完整" : !current.runtimeVerified ? "自动状态尚未连接" :
      current.nativeSections && !current.nativeVerified ? "状态分组尚未确认" : "会话列表已读取 · 自动状态已连接";
    if (current.autoArchiveError) status = "本轮后台自动归档未完成";
    if (current.historyError) status = "部分会话历史暂不可读，保留已有记录";
    if (pendingProject) status = "部分会话的项目归属待确认";
    if (projectError) status = "项目列表暂不可读，保留上次分类";
    if (current.nativeError) status = "状态分组暂不可读，保留上次位置";
    if (partial) status = "部分自动状态更新未完成";
    if (blocked || readonly) status = blocked ? "可以查看会话，暂时不能修改" : "部分会话暂时不能修改";
    if (listError) status = rows.length ? "会话列表暂时无法更新，显示已有记录" : "尚未读取会话，请检查连接";
    if (catalog && !listError && !blocked && !readonly && !faults) status = catalog.error ? "主机目录暂时无法更新，保留上次结果" :
      catalog.pending ? "正在更新主机目录" : catalog.cached && catalog.capturedAt ? "显示上次主机目录快照" :
      !catalog.complete ? "主机目录尚未读取完整" : "主机目录已读取完整";
    if (catalog?.error) status = "主机目录暂时无法更新，保留上次结果";
    if (local && !faults && !blocked && !readonly) {
      if (!projectCatalog.loaded) status = "项目列表尚未读取";
      else if (projectCatalog.pending) status = "正在更新项目列表";
      else if (projectCatalog.incomplete) status = "项目列表尚未读取完整";
      else if (projectCatalog.cached) status = "显示上次项目列表";
    }
    if (idle) status = (host.enabled === false ? "尚未连接" : "主机已移除") + (rows.length ? " · 显示缓存" : " · 尚未读取会话");
    if (catalog?.cached) status += " · 缓存时间：" + date(catalog.capturedAt);
    const effects = [
      "会话列表：" + (idle ? "只显示缓存，尚未确认更新" : listError ? "暂时无法更新，保留已有记录" :
        current.lastSuccessAt ? current.scanComplete ? "已读取" : "尚未读取完整" : "最近读取情况尚未确认"),
      "自动状态：" + (idle || !current.runtimeVerified ? "尚未连接，保留原状态" : partial ? "部分更新未完成" : "已连接"),
      "状态分组：" + (current.nativeSections ? current.nativeError || !current.nativeVerified || idle
        ? "尚未确认更新，保留上次位置" : "已读回" : "不使用原生分组"),
      "修改与归档：" + (idle || blocked ? "暂不可用" : readonly ? "部分会话只读" : "请以卡片上可用的操作为准"),
    ];
    if (projectError || cachedProjects) effects.push("项目资料来自上次记录，暂未确认更新。");
    if (pendingProject) effects.push("有会话的项目归属待确认，可在此主机的全部项目中查看。");
    if (catalog) {
      effects.push("目录快照时间：" + date(catalog.capturedAt));
      if (catalog.pending) effects.push("目录正在刷新，当前先显示上次可用结果。");
    }
    if (current.autoArchiveSeconds)
      effects.push(`后台每 ${current.autoArchiveSeconds / 60} 分钟自动归档，下次 ${date(current.autoArchiveNextAt)}；不受筛选影响。`);
    const projectReady = catalog ? catalog.complete && !catalog.cached && !catalog.pending && !catalog.error && !catalog.issues?.length :
      projectCatalog.loaded && !projectCatalog.error && !projectCatalog.cached && !projectCatalog.incomplete && !projectCatalog.pending;
    const ready = !idle && !attention && projectReady && Boolean(current.lastSuccessAt && current.scanComplete && current.runtimeVerified &&
      (!current.nativeSections || current.nativeVerified)) && !cachedProjects;
    return { key: host.value, label: hostName(host), status, ready, attention, effects, diagnostics,
      lastRead: catalog?.capturedAt || current.lastSuccessAt, canConnect: host.enabled === false && !legacyMode, hostId: host.hostId };
  }

  function healthReports() {
    const { remoteError, remoteCacheErrors } = snapshot().health;
    const selectedHost = sources.selectedHost();
    const reports = hostOptions().map(hostHealth);
    const global = diagnosticsFrom([["远程项目读取", remoteError],
      ["远程缓存读取", remoteCacheErrors?.length ? JSON.stringify(remoteCacheErrors) : null], ["看板服务", sources.readError()]]);
    if (global.length) reports.push({ key: "service", label: "其他读取问题", status: "部分资料未更新", attention: true,
      effects: ["已有记录已保留。这里的问题尚未定位到具体主机，请展开诊断核对。"], diagnostics: global });
    if (selectedHost !== "all" && !reports.some(report => report.key === selectedHost)) reports.unshift({
      key: selectedHost, label: sources.selectedHostLabel(), status: "主机已移除 · 尚未读取会话",
      effects: ["原来的显示范围已保留。请在主机菜单选择另一台主机。"], diagnostics: [], attention: true });
    return reports;
  }

  return { hostHealth, healthReports };
}
