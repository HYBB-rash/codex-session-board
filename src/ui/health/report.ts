import type { CatalogProject, HostCatalog, HostIdentity, HostOption } from "../browse/scope";
import type { BoardSnapshot } from "../controller";
import type { BoardSession } from "../session";

interface HostHealth {
    error?: string | null;
    runtimeError?: string | null;
    nativeError?: string | null;
    deferredSectionError?: string | null;
    nativeStageError?: string | null;
    writeError?: string | null;
    archiveReadError?: string | null;
    archiveMigrationError?: string | null;
    historyError?: string | null;
    lastSuccessAt?: number | null;
    lastScanAt?: number | null;
    runtimeLastAt?: number | null;
    scanComplete?: boolean;
    refreshing?: boolean;
    runtimeVerified?: boolean;
    nativeSections?: boolean;
    nativeVerified?: boolean;
    taskStateError?: string;
    taskStateConfirmedAt?: number;
}

export interface BoardHealth extends HostHealth {
    sshAliasSetup?: boolean;
    nativeConnections?: boolean;
    remoteError?: string | null;
    remoteCacheErrors?: unknown[];
    remoteConnections?: (HostIdentity &
        HostHealth & { connected?: boolean; catalog?: HostCatalog })[];
    remoteConnectionErrors?: (HostIdentity & { error?: string })[];
    remoteHosts?: (HostIdentity & { connectionRequired?: boolean; error?: string })[];
    remoteRegistry?: HostIdentity[];
    unsupportedRemoteHosts?: { hostId: string; hostLabel: string; reason: string }[];
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
    ready?: boolean | undefined;
    attention: boolean;
    effects: string[];
    diagnostics: Diagnostic[];
    lastRead?: number | null | undefined;
    canConnect?: boolean;
    hostId?: string | undefined;
    unsupported?: boolean;
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

function readHostHealth(host: HostOption, sources: HealthSources) {
    const { health, projectCatalog, legacyMode } = sources.snapshot();
    const local = host.value === "local",
        rows = sources.hostSessions(host.value);
    const remote = health.remoteConnections?.find((item) => item.hostId === host.hostId);
    const current: HostHealth = local ? health : (remote ?? {});
    const scan = health.remoteHosts?.find((item) => item.hostId === host.hostId);
    const connection = health.remoteConnectionErrors?.find((item) => item.hostId === host.hostId);
    const idle =
        !local &&
        (host.enabled === false ||
            (!legacyMode && !health.remoteRegistry?.some((item) => item.hostId === host.hostId)));
    const catalog = local ? undefined : remote?.catalog;
    const projectError = local ? projectCatalog.error : catalog?.error;
    const listError =
        current.error || connection?.error || (!scan?.connectionRequired && scan?.error);
    const activeRows = rows.filter((row) => !row.archived && row.presence !== "cachedUnknown");
    const readonly = activeRows.filter((row) => row.readOnly).length;
    const blocked = Boolean(
        current.writeError || (activeRows.length && readonly === activeRows.length),
    );
    const partial = Boolean(
        current.nativeStageError || (current.runtimeVerified && current.runtimeError),
    );
    return {
        host,
        local,
        rows,
        current,
        idle,
        catalog,
        projectCatalog,
        legacyMode,
        projectError,
        listError,
        blocked,
        readonly,
        partial,
    };
}

type HostReading = ReturnType<typeof readHostHealth>;

function summarizeDiagnostics(reading: HostReading) {
    const { catalog, current, projectError, listError, rows, idle, blocked, readonly } = reading;
    const catalogIssues = catalog?.issues?.length
        ? catalog.issues
              .map((issue) => (typeof issue === "string" ? issue : JSON.stringify(issue)))
              .join("\n")
        : null;
    const diagnostics = diagnosticsFrom([
        ["会话列表", listError],
        ["自动状态", current.runtimeError],
        ["状态分组", current.nativeError],
        ["任务记录", current.taskStateError],
        ["待处理", current.deferredSectionError],
        ["状态更新", current.nativeStageError],
        ["修改权限", current.writeError],
        ["项目列表", projectError],
        ["目录归属", catalogIssues],
        ["轮次历史", current.historyError],
        ["归档历史", current.archiveReadError],
        ["旧待归档迁移", current.archiveMigrationError],
    ]);
    if (catalog?.coverage) diagnostics.push(["目录覆盖", JSON.stringify(catalog.coverage)]);
    const incomplete = current.lastScanAt && !current.scanComplete && !current.refreshing;
    const faults = diagnostics.some(
        ([label]) =>
            label !== "目录覆盖" &&
            label !== "待处理" &&
            (label !== "自动状态" ||
                current.lastScanAt ||
                current.lastSuccessAt ||
                current.runtimeLastAt),
    );
    const pendingProject = rows.some((row) => row.projectIssue);
    const attention =
        !idle &&
        Boolean(
            faults ||
                current.deferredSectionError ||
                blocked ||
                readonly ||
                incomplete ||
                (catalog && !catalog.complete) ||
                pendingProject,
        );
    return { diagnostics, faults, pendingProject, attention };
}

type HostDiagnostics = ReturnType<typeof summarizeDiagnostics>;

function sessionReadStatus(current: HostHealth, hasSessions: boolean) {
    if (!current.lastSuccessAt)
        return hasSessions ? "显示已有会话，更新时间待确认" : "正在等待读取会话";
    if (!current.scanComplete) return "会话列表尚未读取完整";
    if (!current.runtimeVerified) return "自动状态尚未连接";
    if (current.nativeSections && !current.nativeVerified) return "状态分组尚未确认";
    return "会话列表已读取 · 自动状态已连接";
}

function sessionStatus(reading: HostReading, diagnostic: HostDiagnostics) {
    const { current, rows, projectError, partial, blocked, readonly, listError } = reading;
    if (listError)
        return rows.length ? "会话列表暂时无法更新，显示已有记录" : "尚未读取会话，请检查连接";
    if (blocked) return "可以查看会话，暂时不能修改";
    if (readonly) return "部分会话暂时不能修改";
    if (partial) return "部分自动状态更新未完成";
    if (current.nativeError) return "状态分组暂不可读，保留上次位置";
    if (projectError) return "项目列表暂不可读，保留上次分类";
    if (diagnostic.pendingProject) return "部分会话的项目归属待确认";
    if (current.historyError) return "部分会话历史暂不可读，保留已有记录";
    return sessionReadStatus(current, rows.length > 0);
}

function projectReadStatus(catalog: BoardSnapshot["projectCatalog"]) {
    if (!catalog.loaded) return "项目列表尚未读取";
    if (catalog.pending) return "正在更新项目列表";
    if (catalog.incomplete) return "项目列表尚未读取完整";
    if (catalog.cached) return "显示上次项目列表";
    return undefined;
}

function catalogReadStatus(catalog: HostCatalog) {
    if (catalog.pending) return "正在更新主机目录";
    if (catalog.cached && catalog.capturedAt) return "显示上次主机目录快照";
    if (!catalog.complete) return "主机目录尚未读取完整";
    return "主机目录已读取完整";
}

function primaryHostStatus(reading: HostReading, diagnostic: HostDiagnostics): string {
    const { host, local, idle, rows, catalog, projectCatalog, listError, blocked, readonly } =
        reading;
    if (idle)
        return `${host.enabled === false ? "尚未连接" : "主机已移除"}${rows.length ? " · 显示缓存" : " · 尚未读取会话"}`;
    const canUseCatalog = !diagnostic.faults && !blocked && !readonly;
    if (local && canUseCatalog) {
        const project = projectReadStatus(projectCatalog);
        if (project) return project;
    }
    if (catalog?.error) return "主机目录暂时无法更新，保留上次结果";
    if (catalog && canUseCatalog && !listError) return catalogReadStatus(catalog);
    return sessionStatus(reading, diagnostic);
}

function hostStatus(
    reading: HostReading,
    diagnostic: HostDiagnostics,
    date: HealthSources["date"],
) {
    const { catalog } = reading;
    const status = primaryHostStatus(reading, diagnostic);
    return catalog?.cached ? `${status} · 缓存时间：${date(catalog.capturedAt)}` : status;
}

function sessionListEffect({ idle, listError, current }: HostReading) {
    if (idle) return "只显示缓存，尚未确认更新";
    if (listError) return "暂时无法更新，保留已有记录";
    if (!current.lastSuccessAt) return "最近读取情况尚未确认";
    return current.scanComplete ? "已读取" : "尚未读取完整";
}

function sectionReadEffect({ idle, current }: HostReading) {
    if (!current.nativeSections) return "不使用原生分组";
    if (current.nativeError || !current.nativeVerified || idle) return "尚未确认更新，保留上次位置";
    return "已读回";
}

function readingEffects(reading: HostReading) {
    const { idle, current, partial, blocked, readonly } = reading;
    const runtime =
        idle || !current.runtimeVerified
            ? "尚未连接，保留原状态"
            : partial
              ? "部分更新未完成"
              : "已连接";
    const changes =
        idle || blocked ? "暂不可用" : readonly ? "部分会话只读" : "请以卡片上可用的操作为准";
    return [
        `会话列表：${sessionListEffect(reading)}`,
        `自动状态：${runtime}`,
        `状态分组：${sectionReadEffect(reading)}`,
        `修改与归档：${changes}`,
    ];
}

function hostEffects(
    reading: HostReading,
    diagnostic: HostDiagnostics,
    cachedProjects: boolean,
    date: HealthSources["date"],
) {
    const { catalog, projectError } = reading;
    const effects = readingEffects(reading);
    if (projectError || cachedProjects) effects.push("项目资料来自上次记录，暂未确认更新。");
    if (diagnostic.pendingProject)
        effects.push("有会话的项目归属待确认，可在此主机的全部项目中查看。");
    if (catalog) {
        effects.push(`目录快照时间：${date(catalog.capturedAt)}`);
        if (catalog.pending) effects.push("目录正在刷新，当前先显示上次可用结果。");
    }
    return effects;
}

function isReady(reading: HostReading, diagnostic: HostDiagnostics, cachedProjects: boolean) {
    const { catalog, current, idle, projectCatalog } = reading;
    const projectReady = catalog
        ? catalog.complete &&
          !catalog.cached &&
          !catalog.pending &&
          !catalog.error &&
          !catalog.issues?.length
        : projectCatalog.loaded &&
          !projectCatalog.error &&
          !projectCatalog.cached &&
          !projectCatalog.incomplete &&
          !projectCatalog.pending;
    return (
        !idle &&
        !diagnostic.attention &&
        projectReady &&
        Boolean(
            current.lastSuccessAt &&
                current.scanComplete &&
                current.runtimeVerified &&
                (!current.nativeSections || current.nativeVerified),
        ) &&
        !cachedProjects
    );
}

function hostHealth(host: HostOption, sources: HealthSources): HealthReport {
    const reading = readHostHealth(host, sources);
    const diagnostic = summarizeDiagnostics(reading);
    const cachedProjects = sources.hostProjects(host.value).some((item) => item.cached);
    return {
        key: host.value,
        label: sources.hostName(host),
        status: hostStatus(reading, diagnostic, sources.date),
        ready: isReady(reading, diagnostic, cachedProjects),
        attention: diagnostic.attention,
        effects: hostEffects(reading, diagnostic, cachedProjects, sources.date),
        diagnostics: diagnostic.diagnostics,
        lastRead: reading.catalog?.capturedAt || reading.current.lastSuccessAt,
        canConnect: host.enabled === false && !reading.legacyMode,
        hostId: host.hostId,
    };
}

function healthReports(sources: HealthSources) {
    const { remoteError, remoteCacheErrors, unsupportedRemoteHosts } = sources.snapshot().health;
    const selectedHost = sources.selectedHost();
    const reports = sources.hostOptions().map((host) => hostHealth(host, sources));
    for (const { hostId, hostLabel, reason } of unsupportedRemoteHosts ?? []) {
        reports.push({
            key: hostId,
            label: hostLabel,
            status: reason,
            attention: true,
            effects: [],
            diagnostics: [],
            unsupported: true,
        });
    }
    const global = diagnosticsFrom([
        ["远程项目读取", remoteError],
        ["远程缓存读取", remoteCacheErrors?.length ? JSON.stringify(remoteCacheErrors) : null],
        ["看板服务", sources.readError()],
    ]);
    if (global.length)
        reports.push({
            key: "service",
            label: "其他读取问题",
            status: "部分资料未更新",
            attention: true,
            effects: ["已有记录已保留。这里的问题尚未定位到具体主机，请展开诊断核对。"],
            diagnostics: global,
        });
    if (selectedHost !== "all" && !reports.some((report) => report.key === selectedHost))
        reports.unshift({
            key: selectedHost,
            label: sources.selectedHostLabel(),
            status: "主机已移除 · 尚未读取会话",
            effects: ["原来的显示范围已保留。请在主机菜单选择另一台主机。"],
            diagnostics: [],
            attention: true,
        });
    return reports;
}

export function createBoardHealthReports(sources: HealthSources) {
    return {
        hostHealth: (host: HostOption) => hostHealth(host, sources),
        healthReports: () => healthReports(sources),
    };
}
