import {
    finalAnswer,
    type Phase,
    phaseIsFromTurn,
    projectedTitle,
    runtimePhase,
    turnInput,
} from "./activity.js";
import {
    Archive,
    type ArchiveItem,
    ArchivePreconditionError,
    type ArchiveTarget,
    type BoardLedger,
    emptyLedger,
    NewInputError,
    type SortPreference,
} from "./archive.js";
import {
    cardId,
    type DesktopConnection,
    type HostConnection,
    type HostId,
    type HostProject,
    interactiveThread,
    localHostId,
    type SidebarContext,
    type Thread,
    type ThreadId,
} from "./desktop-connection.js";
import { type DraftInput, type DraftRecord, stageDraft, unresolvedDrafts } from "./drafts.js";
import {
    type BoardState,
    type Placement,
    placement,
    presentState,
    sectionBindings,
    sectionFor,
    visibleStates,
} from "./placement.js";
import { previewReply } from "./preview.js";
import type { CardRecord, PhaseIntent, RecordWrite, TitleResolution } from "./records.js";
import { type BoardSettings, defaults } from "./settings.js";

interface BoardPorts {
    load(account: string): Promise<Record<string, CardRecord>>;
    save(update: RecordWrite): Promise<void>;
    preparePhase(update: PhaseIntent): Promise<boolean>;
    settleTitle(resolution: TitleResolution): Promise<boolean>;
    loadLedger(account: string): Promise<BoardLedger>;
    writeLedger(account: string, ledger: BoardLedger): Promise<void>;
    settings(account: string): Promise<BoardSettings>;
    titleEpoch(account: string): Promise<number>;
    closePage(): void;
    authority(account: string | undefined): void;
    updateStatus(start: boolean): Promise<unknown>;
    proof(stage: string, detail: Record<string, unknown>): void;
}

interface HostSnapshot {
    active: Map<ThreadId, Thread>;
    archived: Map<ThreadId, Thread>;
    currentActive: Set<ThreadId>;
    currentArchived: Set<ThreadId>;
    projects: Map<string, HostProject>;
    connection?: HostConnection;
    error?: string;
    lastRead?: number;
    scanComplete?: boolean;
    projectRead?: boolean;
    projectError?: string;
    archiveError?: string;
    runtimeError?: string;
    runtimeVerified?: boolean;
    historyError?: string;
}

interface HistoryRead {
    sequence: number;
    error?: string;
}

interface AutoArchiveRun {
    at: number;
    archived: string[];
    deferred: { id: string; reason: string }[];
    error?: string;
}

interface Edit {
    id: string;
    state: BoardState | null;
    note?: string;
    version: number;
    inputVersion: number;
    sectionVersion: string | null;
}

interface Card {
    id: string;
    hostId: HostId;
    threadId: Thread["id"];
    title: string;
    cwd: string;
    projectId: string | null;
    updatedAt: number;
    state: BoardState | null;
    sectionVersion: string;
    version: number;
    inputVersion: number;
    note: string;
    manual: boolean;
    remote: boolean;
    ordinary: boolean;
    canOpen: boolean;
    readOnly: boolean;
    presence: "present" | "cachedUnknown";
    catalogCurrent: boolean;
    archived: boolean;
    source: "codex";
    followUp: boolean;
    canArchive: boolean;
    hostLabel: string;
    previewTurnId?: string;
    reason?: string;
    titleSyncPending?: boolean;
    titleSyncError?: string;
    observationError?: string;
    observationWarning?: string;
    archivePending: boolean;
}

const emptyRecord: CardRecord = { note: "", version: 0, inputVersion: 0 };
const titleRpcBudgetMs = 3000;
const titleRetryDelayMs = 30000;
const historyPollBudgetMs = 3000;
const emptyHost = (): HostSnapshot => ({
    active: new Map(),
    archived: new Map(),
    currentActive: new Set(),
    currentArchived: new Set(),
    projects: new Map(),
});
const errorText = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);
const accountKey = (context: SidebarContext): string =>
    JSON.stringify([context.accountId, context.userId]);
const hostLabel = (host: HostId): string =>
    host === "local" ? "本机" : (host.split(":").pop() ?? host);

function projectKey(hostId: HostId, id: string): string {
    return hostId === "local" ? id : `target:${JSON.stringify([hostId, "targetServer", id])}`;
}

function changedInput(current: CardRecord, input: string | null): boolean {
    return input !== null && input !== current.inputKey;
}

function remainingTitleTime(deadline: number): number {
    const remaining = deadline - performance.now();
    if (remaining < 1) throw new Error("Codex 标题同步超时");
    return Math.floor(remaining);
}

function isNewPhase(phase: Phase, current: CardRecord, since: number): boolean {
    if (phaseIsFromTurn(current.pendingPhaseKey, phase.turnId)) return true;
    if (phase.key === current.phaseKey) return false;
    const sameTurn = phaseIsFromTurn(current.phaseKey, phase.turnId);
    if (phase.terminal && (phase.at ?? 0) <= Math.floor(current.selectedAt ?? 0)) return false;
    return sameTurn || (phase.at !== null && phase.at >= Math.floor(since));
}

export class BoardService {
    private readonly hosts = new Map<HostId, HostSnapshot>();
    private context?: SidebarContext;
    private records: Record<string, CardRecord> = {};
    private refreshing: Promise<void> | undefined;
    private readonly editing = new Set<string>();
    private settings: BoardSettings = defaults;
    private titleStartedAt: number | undefined;
    private readonly observationSince = Date.now() / 1000;
    private readonly runtimeDirty = new Map<HostId, { epoch: number; ids: Set<ThreadId> }>();
    private readonly historyReads = new Map<string, HistoryRead>();
    private historySequence = 0;
    private readonly titleInFlight = new Set<string>();
    private readonly nextTitleAttempt = new Map<string, { version: number; at: number }>();
    private readonly changed = new Map<string, number>();
    private changes = 0;
    private ledger: BoardLedger = emptyLedger();
    private readonly archive = new Archive({
        scope: () => accountKey(this.requiredContext()),
        ledger: () => this.ledger,
        write: (next) => this.writeLedger(next),
        verify: async (item) => {
            await this.verifyArchive(item);
        },
        isArchived: (id) => this.archived(id),
        archive: (item) => this.archiveThread(item),
    });
    private readonly archiveDeadlines = new Map<HostId, number>();
    private readonly archiveRuns = new Map<HostId, AutoArchiveRun>();
    private get mode() {
        return this.settings.mode;
    }

    constructor(
        private readonly desktop: DesktopConnection,
        private readonly ports: BoardPorts,
    ) {}

    async refresh(): Promise<void> {
        this.refreshing ??= this.readBoard()
            .catch((error: unknown) => {
                this.ports.authority(undefined);
                throw error;
            })
            .finally(() => {
                this.refreshing = undefined;
            });
        await this.refreshing;
    }

    private async readBoard(): Promise<void> {
        const context = await this.desktop.context();
        const account = accountKey(context);
        const settings = await this.ports.settings(account);
        const titleStartedAt = settings.syncTitles
            ? await this.ports.titleEpoch(account)
            : undefined;
        if (!this.context || account !== accountKey(this.context)) {
            this.hosts.clear();
            this.changed.clear();
            this.runtimeDirty.clear();
            this.historyReads.clear();
            this.nextTitleAttempt.clear();
            [this.records, this.ledger] = await Promise.all([
                this.ports.load(account),
                this.ports.loadLedger(account),
            ]);
            this.archiveDeadlines.clear();
            this.archiveRuns.clear();
        }
        this.settings = settings;
        this.titleStartedAt = titleStartedAt;
        this.context = context;
        this.ports.authority(account);
        await Promise.all(this.desktop.knownHosts().map((host) => this.refreshHost(host, context)));
        this.autoArchive(context);
    }

    private async readPages(
        connection: HostConnection,
        archived: boolean,
        rows: Map<ThreadId, Thread>,
    ): Promise<void> {
        const cursors = new Set<string>();
        let cursor: string | null = null;
        for (let index = 0; index < this.settings.maxPages; index++) {
            const page = await this.desktop.list(
                connection,
                archived,
                cursor,
                this.settings.pageSize,
            );
            for (const thread of page.data) {
                if (interactiveThread(thread) && !rows.has(thread.id)) rows.set(thread.id, thread);
            }
            cursor = page.nextCursor;
            if (!cursor) return;
            if (cursors.has(cursor)) throw new Error("Codex 返回了重复的分页游标");
            cursors.add(cursor);
        }
        throw new Error(`会话列表达到分页上限（${this.settings.maxPages} 页）`);
    }

    private async refreshHost(hostId: HostId, context: SidebarContext): Promise<void> {
        const last = this.hosts.get(hostId) ?? emptyHost();
        const next: HostSnapshot = {
            ...emptyHost(),
            archived: last.archived,
            projects: last.projects,
        };
        const scanVersion = this.changes;
        try {
            const connection = this.desktop.capture(hostId);
            try {
                await this.readPages(connection, false, next.active);
                next.scanComplete = true;
            } catch (error) {
                if (!next.active.size) throw error;
                next.error = errorText(error);
            }
            await this.freshContext();
            this.desktop.assertCurrent(connection);
            next.currentActive = new Set(next.active.keys());
            const archiveEpochMatches = last.connection?.epoch === connection.epoch;
            if (archiveEpochMatches) next.currentArchived = new Set(last.currentArchived);
            this.retainMissing(last.active, next.active);
            this.mergeLateWrites(hostId, last, next, scanVersion);
            next.connection = connection;
            const lastRead = next.scanComplete ? Date.now() / 1000 : last.lastRead;
            if (lastRead !== undefined) next.lastRead = lastRead;
            this.hosts.set(hostId, next);
            await Promise.all([
                this.refreshArchives(connection, next),
                this.refreshProjects(connection, next),
                this.observeHost(connection, next, context),
            ]);
        } catch (error) {
            const available = this.hosts.get(hostId) ?? last;
            this.hosts.set(hostId, {
                ...emptyHost(),
                active: available.active,
                archived: available.archived,
                projects: available.projects,
                ...(this.isLive(available) && available.connection
                    ? { connection: available.connection }
                    : {}),
                ...(available.lastRead === undefined ? {} : { lastRead: available.lastRead }),
                scanComplete: false,
                error: errorText(error),
            });
        }
    }

    private retainMissing(previous: Map<ThreadId, Thread>, result: Map<ThreadId, Thread>): void {
        for (const [id, thread] of previous) if (!result.has(id)) result.set(id, thread);
    }

    private mergeLateWrites(
        hostId: HostId,
        old: HostSnapshot,
        next: HostSnapshot,
        version: number,
    ): void {
        for (const thread of old.active.values()) {
            const id = cardId({ hostId, threadId: thread.id });
            if ((this.changed.get(id) ?? 0) > version) {
                next.active.set(thread.id, thread);
                next.currentActive.add(thread.id);
            }
        }
    }

    private async refreshArchives(connection: HostConnection, host: HostSnapshot): Promise<void> {
        try {
            const archived = new Map<ThreadId, Thread>();
            await this.readPages(connection, true, archived);
            await this.freshContext();
            this.desktop.assertCurrent(connection);
            this.mergeArchives(host, archived);
        } catch (error) {
            host.currentArchived = new Set();
            host.archiveError = errorText(error);
        }
    }

    private mergeArchives(host: HostSnapshot, archived: Map<ThreadId, Thread>): void {
        host.currentArchived = new Set(archived.keys());
        for (const id of host.currentActive) {
            archived.delete(id);
            host.currentArchived.delete(id);
        }
        for (const id of host.currentArchived) host.active.delete(id);
        for (const [id, thread] of host.archived) {
            if (!host.currentActive.has(thread.id) && !archived.has(id)) archived.set(id, thread);
        }
        host.archived = archived;
    }

    private async refreshProjects(connection: HostConnection, host: HostSnapshot): Promise<void> {
        try {
            const projects = new Map<string, HostProject>();
            const cursors = new Set<string>();
            let cursor: string | null = null;
            do {
                const page = await this.desktop.projects(connection, cursor);
                for (const project of page.data) projects.set(project.id, project);
                cursor = page.nextCursor;
                if (cursor && cursors.has(cursor)) throw new Error("项目分页游标重复");
                if (cursor) cursors.add(cursor);
            } while (cursor);
            await this.freshContext();
            this.desktop.assertCurrent(connection);
            host.projects = projects;
            host.projectRead = true;
        } catch (error) {
            host.projectError = errorText(error);
        }
    }

    notice(hostId: HostId, threadId: ThreadId): void {
        let connection: HostConnection;
        try {
            connection = this.desktop.capture(hostId);
        } catch {
            return;
        }
        const previous = this.runtimeDirty.get(hostId);
        const ids = previous?.epoch === connection.epoch ? previous.ids : new Set<ThreadId>();
        ids.add(threadId);
        this.runtimeDirty.set(hostId, { epoch: connection.epoch, ids });
    }

    private async observeHost(
        connection: HostConnection,
        host: HostSnapshot,
        context: SidebarContext,
    ): Promise<void> {
        const pending = this.runtimeDirty.get(connection.hostId);
        const dirty = pending?.epoch === connection.epoch ? pending.ids : new Set<ThreadId>();
        this.runtimeDirty.delete(connection.hostId);
        const threads = [...host.active.values()].filter(
            (thread) => host.currentActive.has(thread.id) && this.canWrite(thread.id),
        );
        const realtime = threads.filter(
            (thread, index) =>
                dirty.has(thread.id) ||
                thread.status?.type === "active" ||
                this.records[cardId({ hostId: connection.hostId, threadId: thread.id })]
                    ?.pendingPhaseKey !== undefined ||
                (index < 12 && thread.updatedAt >= this.observationSince - 3),
        );
        for (const thread of realtime) {
            try {
                await this.observeThread(connection, thread, context);
            } catch (error) {
                host.runtimeError = errorText(error);
                if (dirty.has(thread.id)) this.notice(connection.hostId, thread.id);
            }
        }
        host.runtimeVerified = host.scanComplete === true && host.runtimeError === undefined;
        await this.retryHostTitle(connection, host);
        const checked = new Set(realtime.map(({ id }) => id));
        await this.readHistory(
            connection,
            host,
            context,
            threads.filter(({ id }) => !checked.has(id)),
        );
    }

    private async readHistory(
        connection: HostConnection,
        host: HostSnapshot,
        context: SidebarContext,
        candidates: Thread[],
    ): Promise<void> {
        const sequence = (thread: Thread) =>
            this.historyReads.get(cardId({ hostId: connection.hostId, threadId: thread.id }))
                ?.sequence ?? 0;
        candidates.sort((left, right) => sequence(left) - sequence(right));
        const deadline = performance.now() + historyPollBudgetMs;
        for (const thread of candidates.slice(0, this.settings.historyPerPoll)) {
            const timeout = Math.floor(deadline - performance.now());
            if (timeout < 1) break;
            try {
                this.desktop.assertCurrent(connection);
                await this.observeThread(connection, thread, context, { historyTimeout: timeout });
            } catch (error) {
                host.historyError = errorText(error);
            }
        }
        const errors = [...host.currentActive].filter(
            (threadId) =>
                this.historyReads.get(cardId({ hostId: connection.hostId, threadId }))?.error,
        ).length;
        if (errors) host.historyError = `${errors} 个会话的历史暂不可读`;
    }

    private async observeThread(
        connection: HostConnection,
        thread: Thread,
        context: SidebarContext,
        options: { historyTimeout?: number } = {},
    ): Promise<void> {
        const id = cardId({ hostId: connection.hostId, threadId: thread.id });
        if (this.editing.has(id)) {
            if (options.historyTimeout === undefined) this.notice(connection.hostId, thread.id);
            return;
        }
        this.editing.add(id);
        let queuedTitle = false;
        try {
            const latest = await this.readObservedTurn(
                connection,
                thread.id,
                id,
                options.historyTimeout,
            );
            const current = this.records[id] ?? emptyRecord;
            const input = turnInput(latest);
            const phase =
                options.historyTimeout === undefined ? runtimePhase(thread.status, latest) : null;
            const inputChanged = changedInput(current, input);
            const phaseChanged =
                phase !== null &&
                (current.phaseKey !== phase.key ||
                    phaseIsFromTurn(current.pendingPhaseKey, phase.turnId));
            const answer = latest && finalAnswer(latest) ? latest.id : undefined;
            if (!inputChanged && !phaseChanged && (!answer || answer === current.previewTurnId))
                return;
            const next = {
                ...current,
                ...(input !== null ? { inputKey: input } : {}),
                version: current.version + 1,
                inputVersion: current.inputVersion + Number(inputChanged),
                ...(phase ? { phaseKey: phase.key } : {}),
                ...(answer ? { previewTurnId: answer } : {}),
            };
            if (phase && isNewPhase(phase, current, this.observationSince))
                await this.applyPhase(connection, thread, context, phase, next);
            if (phaseChanged) delete next.pendingPhaseKey;
            await this.commitRecord(context, id, current.version, next);
            queuedTitle = next.title !== undefined && next.title !== current.title;
        } finally {
            this.editing.delete(id);
        }
        if (queuedTitle) void this.retryTitle(connection, thread.id);
    }

    private async readObservedTurn(
        connection: HostConnection,
        threadId: ThreadId,
        id: string,
        timeout?: number,
    ) {
        const sequence = ++this.historySequence;
        try {
            const latest = (
                await this.desktop.turns(connection, threadId, "summary", null, timeout)
            ).data[0];
            this.historyReads.set(id, { sequence });
            return latest;
        } catch (error) {
            this.historyReads.set(id, { sequence, error: errorText(error) });
            throw error;
        }
    }

    private async applyPhase(
        connection: HostConnection,
        thread: Thread,
        context: SidebarContext,
        phase: Phase,
        next: CardRecord,
    ): Promise<void> {
        const position = placement(context, connection.hostId, thread);
        if (!position.managed) return;
        const current = await this.desktop.read(connection, thread.id, false);
        if (placement(context, connection.hostId, current).version !== position.version) return;
        const id = cardId({ hostId: connection.hostId, threadId: thread.id });
        const fresh = await this.freshContext();
        this.desktop.assertCurrent(connection);
        const account = accountKey(context);
        if (account !== accountKey(fresh)) throw new Error("Codex 账号已变更");
        const pending = {
            account,
            id,
            expected: next.version - 1,
            phaseKey: phase.key,
        };
        if (!(await this.ports.preparePhase(pending)))
            throw new Error("自动阶段已过期，请刷新后重试");
        this.desktop.assertCurrent(connection);
        if (account !== accountKey(this.requiredContext())) throw new Error("Codex 账号已变更");
        this.records[id] = { ...(this.records[id] ?? emptyRecord), pendingPhaseKey: phase.key };
        const after = await this.moveCard(phase.state, connection, context, current);
        next.manual = false;
        next.reason = phase.reason;
        const afterActivation =
            !phase.terminal ||
            (phase.at !== null &&
                this.titleStartedAt !== undefined &&
                phase.at >= Math.floor(this.titleStartedAt));
        if (this.settings.syncTitles && (afterActivation || next.title))
            next.title = { state: phase.state };
        this.publishThread(connection.hostId, after);
    }

    private async retryHostTitle(connection: HostConnection, host: HostSnapshot): Promise<void> {
        if (!this.settings.syncTitles) return;
        const now = Date.now();
        let oldest: { threadId: ThreadId; at: number } | undefined;
        for (const thread of host.active.values()) {
            const id = cardId({ hostId: connection.hostId, threadId: thread.id });
            const record = this.records[id];
            if (
                !host.currentActive.has(thread.id) ||
                !record?.title ||
                !this.canWrite(thread.id) ||
                this.editing.has(id) ||
                this.titleInFlight.has(this.titleKey(id))
            )
                continue;
            const retry = this.nextTitleAttempt.get(id);
            const at = retry?.version === record.version ? retry.at : 0;
            if (at <= now && (!oldest || at < oldest.at)) oldest = { threadId: thread.id, at };
        }
        if (oldest) await this.retryTitle(connection, oldest.threadId);
    }

    private isLive(host: HostSnapshot): boolean {
        if (!host.connection) return false;
        try {
            this.desktop.assertCurrent(host.connection);
            return true;
        } catch {
            return false;
        }
    }

    private canWrite(threadId: ThreadId): boolean {
        return this.settings.writableThreadIds?.includes(threadId) ?? true;
    }

    private card(
        hostId: HostId,
        thread: Thread,
        context: SidebarContext,
        live: boolean,
        archived = false,
    ): Card {
        const id = cardId({ hostId, threadId: thread.id });
        const record = this.records[id] ?? emptyRecord;
        const position = placement(context, hostId, thread);
        const state = presentState(context, position, this.mode);
        return {
            id,
            hostId,
            threadId: thread.id,
            title: thread.name || thread.preview || "未命名会话",
            cwd: thread.cwd,
            projectId: thread.projectId ? projectKey(hostId, thread.projectId) : null,
            updatedAt: thread.updatedAt,
            state,
            sectionVersion: position.version,
            version: record.version,
            inputVersion: record.inputVersion,
            note: record.note,
            manual: record.manual === true,
            remote: hostId !== "local",
            ordinary: true,
            canOpen: live,
            readOnly: !live || !position.managed || archived || !this.canWrite(thread.id),
            archived,
            presence: live ? "present" : "cachedUnknown",
            catalogCurrent: live,
            source: "codex",
            hostLabel: hostLabel(hostId),
            followUp:
                record.manual === true &&
                record.confirmedInputKey !== undefined &&
                record.confirmedInputKey !== record.inputKey,
            canArchive:
                live &&
                position.managed &&
                !archived &&
                this.mode !== "legacy" &&
                this.canWrite(thread.id),
            archivePending: this.archive.pending(id),
            ...this.cardObservation(id, record, position),
        };
    }

    private cardObservation(id: string, record: CardRecord, position: Placement) {
        const observationError = this.historyReads.get(id)?.error;
        return {
            ...(record.previewTurnId ? { previewTurnId: record.previewTurnId } : {}),
            ...(record.reason ? { reason: record.reason } : {}),
            ...(record.title ? { titleSyncPending: true } : {}),
            ...(record.title?.error ? { titleSyncError: record.title.error } : {}),
            ...(observationError ? { observationError } : {}),
            ...(position.sectionId === null &&
            position.managed &&
            record.manual !== true &&
            record.previewTurnId !== undefined
                ? { observationWarning: "历史轮次已有最终回复，可归入待审阅" }
                : {}),
        };
    }

    private requiredContext(): SidebarContext {
        if (!this.context) throw new Error("看板尚未读取当前账号");
        return this.context;
    }

    private archiveInterval(hostId: string) {
        return hostId === "local"
            ? this.settings.autoArchiveSeconds
            : this.settings.remoteAutoArchiveSeconds;
    }

    private health(hostId: HostId, host: HostSnapshot | undefined) {
        const connected = host !== undefined && this.isLive(host);
        const run = this.archiveRuns.get(hostId);
        const bindings =
            connected && this.context ? sectionBindings(this.context, hostId, this.mode) : [];
        const missingSections = Object.entries(visibleStates(this.mode))
            .filter(([state]) => !bindings.some((binding) => binding.state === state))
            .map(([, label]) => label);
        return {
            lastScanAt: Date.now() / 1000,
            lastSuccessAt: host?.lastRead,
            error: host?.error,
            connected,
            scanComplete: connected && host?.scanComplete === true,
            nativeSections: true,
            nativeVerified: connected && missingSections.length === 0,
            sectionBindings: bindings,
            nativeError:
                connected && missingSections.length
                    ? `Codex 侧栏缺少可确认的状态分区：${missingSections.join("、")}`
                    : undefined,
            runtimeVerified: connected && host?.runtimeVerified === true,
            runtimeError: host?.runtimeError,
            autoArchiveSeconds: this.archiveInterval(hostId),
            autoArchiveNextAt: this.archiveDeadlines.get(hostId),
            autoArchiveLastRun: run,
            autoArchiveError: run?.error,
            archiveReadError: host?.archiveError,
            historyError: host?.historyError,
            catalog: {
                complete: connected && host?.projectRead === true,
                cached: !connected || host?.projectRead !== true,
                capturedAt: host?.lastRead,
                error: host?.projectError,
            },
        };
    }

    snapshot() {
        const context = this.requiredContext();
        const local = this.hosts.get(localHostId);
        const remoteHosts = [...this.hosts.keys()]
            .filter((key) => key !== "local")
            .map((key) => ({ hostId: key, hostLabel: hostLabel(key) }));
        return {
            states: visibleStates(this.mode),
            sessions: [...this.hosts].flatMap(([hostId, host]) =>
                this.hostCards(hostId, host, context),
            ),
            health: {
                ...this.health(localHostId, local),
                nativeConnections: true,
                remoteRegistry: remoteHosts,
                remoteHosts,
                remoteConnections: remoteHosts.map((host) => ({
                    ...host,
                    ...this.health(host.hostId, this.hosts.get(host.hostId)),
                })),
            },
            projectCatalog: {
                loaded: [...this.hosts.values()].some(
                    (host) => host.projectRead || host.projects.size,
                ),
                items: this.projectCatalog(),
                remoteHosts,
                error: local?.projectError,
                cached: local?.projectRead !== true || !this.isLive(local),
            },
            fourColumns: this.mode === "four",
            legacyMode: this.mode === "legacy",
            columnSorts: this.ledger.columnSorts,
            pendingDrafts: unresolvedDrafts(this.ledger).filter(
                ({ source }) => source === "legacy",
            ),
        };
    }

    private hostCards(hostId: HostId, host: HostSnapshot, context: SidebarContext): Card[] {
        const connected = this.isLive(host);
        return [
            ...[...host.active.values()].map((thread) =>
                this.card(hostId, thread, context, connected && host.currentActive.has(thread.id)),
            ),
            ...[...host.archived.values()]
                .filter((thread) => !host.active.has(thread.id))
                .map((thread) =>
                    this.card(
                        hostId,
                        thread,
                        context,
                        connected && host.currentArchived.has(thread.id),
                        true,
                    ),
                ),
        ];
    }

    private projectCatalog() {
        return [...this.hosts].flatMap(([hostId, host]) =>
            [...host.projects.values()].map((project) => ({
                id: projectKey(hostId, project.id),
                hostId,
                hostLabel: hostLabel(hostId),
                remote: hostId !== "local",
                label: project.name,
                paths: project.roots.map(({ path }) => path),
                cached: host.projectRead !== true || !this.isLive(host),
                canCreate: host.projectRead === true && this.isLive(host),
                ...(hostId === "local" ? {} : { origin: "targetServer" }),
            })),
        );
    }

    private target(id: string) {
        const card = this.snapshot().sessions.find((row) => row.id === id);
        if (!card) throw new Error("看板找不到该会话");
        return { card, connection: this.desktop.capture(card.hostId) };
    }

    private async freshContext(): Promise<SidebarContext> {
        const context = await this.desktop.context();
        if (accountKey(context) !== accountKey(this.requiredContext()))
            throw new Error("Codex 账号已变更");
        return context;
    }

    async update(edit: Edit) {
        if (this.editing.has(edit.id)) throw new Error("该卡片正在保存");
        this.editing.add(edit.id);
        let saved: Awaited<ReturnType<BoardService["updateCard"]>>;
        try {
            saved = await this.updateCard(edit);
        } finally {
            this.editing.delete(edit.id);
        }
        const { connection, account, settled, titleEnabled } = saved;
        if (
            settled.titleSyncPending &&
            accountKey(this.requiredContext()) === account &&
            this.records[edit.id]?.version === settled.version
        )
            await this.retryTitle(connection, settled.threadId);
        const context = this.requiredContext();
        const current =
            accountKey(context) === account && this.records[edit.id]?.version === settled.version
                ? this.hosts.get(settled.hostId)?.active.get(settled.threadId)
                : undefined;
        const session = current ? this.card(settled.hostId, current, context, true) : settled;
        return {
            ok: true as const,
            session,
            titleEnabled,
            titlePending: session.titleSyncPending,
            titleError: session.titleSyncError,
        };
    }

    private async resolveEdit(edit: Edit) {
        const { card, connection } = this.target(edit.id);
        if (card.readOnly) throw new Error("此会话目前只读");
        const context = await this.freshContext();
        const before = await this.desktop.read(connection, card.threadId, false);
        const latest = (await this.desktop.turns(connection, card.threadId, "summary")).data[0];
        const from = placement(context, card.hostId, before);
        const current = this.records[edit.id] ?? emptyRecord;
        const inputKey = turnInput(latest) ?? current.inputKey ?? null;
        const newInput =
            inputKey !== null &&
            inputKey !== current.inputKey &&
            (current.inputKey !== undefined || (latest?.startedAt ?? 0) > card.updatedAt);
        if (newInput) {
            await this.commitRecord(context, card.id, current.version, {
                ...current,
                inputKey,
                version: current.version + 1,
                inputVersion: current.inputVersion + 1,
            });
            this.publishThread(card.hostId, before);
            throw new Error("卡片位置或输入已更新，请刷新后重试");
        }
        if (
            !from.managed ||
            edit.version !== current.version ||
            edit.inputVersion !== current.inputVersion ||
            edit.sectionVersion !== from.version
        ) {
            throw new Error("卡片位置或输入已更新，请刷新后重试");
        }
        return { card, connection, context, before, latest, current, inputKey };
    }

    private async updateCard(edit: Edit) {
        const { card, connection, context, before, latest, current, inputKey } =
            await this.resolveEdit(edit);
        const after = await this.moveCard(edit.state, connection, context, before);
        this.desktop.assertCurrent(connection);
        const nextContext = await this.freshContext();
        const record = {
            note: edit.note ?? current.note,
            version: current.version + 1,
            inputVersion: current.inputVersion,
            ...(inputKey === null ? {} : { inputKey, confirmedInputKey: inputKey }),
            ...(current.phaseKey ? { phaseKey: current.phaseKey } : {}),
            ...(latest && finalAnswer(latest) ? { previewTurnId: latest.id } : {}),
            manual: true,
            selectedAt: Date.now() / 1000,
            ...(this.settings.syncTitles ? { title: { state: edit.state } } : {}),
        };
        await this.commitRecord(context, card.id, current.version, record);
        this.context = nextContext;
        this.publishThread(card.hostId, after);
        return {
            connection,
            account: accountKey(context),
            settled: this.card(card.hostId, after, nextContext, true),
            titleEnabled: this.settings.syncTitles,
        };
    }

    private async commitRecord(
        context: SidebarContext,
        id: string,
        expected: number,
        record: CardRecord,
    ): Promise<void> {
        await this.freshContext();
        await this.ports.save({ account: accountKey(context), id, expected, record });
        if (accountKey(context) !== accountKey(this.requiredContext()))
            throw new Error("保存期间 Codex 账号已变更");
        this.records[id] = record;
    }

    private publishThread(hostId: HostId, thread: Thread): void {
        this.changed.set(cardId({ hostId, threadId: thread.id }), ++this.changes);
        this.hosts.get(hostId)?.active.set(thread.id, thread);
    }

    private async syncTitle(
        connection: HostConnection,
        threadId: ThreadId,
        id: string,
        record: CardRecord,
        state: BoardState | null,
        deadline: number,
    ): Promise<Thread | undefined> {
        await this.freshContext();
        const before = await this.desktop.read(
            connection,
            threadId,
            false,
            remainingTitleTime(deadline),
        );
        if (this.records[id] !== record || this.editing.has(id)) return;
        const current = before.name || before.preview.split("\n")[0] || threadId;
        const title = projectedTitle(current, state);
        if (title === current) return before;
        const context = await this.freshContext();
        if (this.records[id] !== record || this.editing.has(id) || !this.settings.syncTitles)
            return;
        const position = placement(context, connection.hostId, before);
        if (!position.managed || presentState(context, position, this.mode) !== state)
            return before;
        await this.desktop.rename(connection, threadId, title, remainingTitleTime(deadline));
        const after = await this.desktop.read(
            connection,
            threadId,
            false,
            remainingTitleTime(deadline),
        );
        await this.freshContext();
        if (after.name !== title) throw new Error("Codex 尚未读回更新后的标题");
        return after;
    }

    private titleKey(id: string): string {
        return JSON.stringify([accountKey(this.requiredContext()), id]);
    }

    private async retryTitle(connection: HostConnection, threadId: ThreadId): Promise<void> {
        if (!this.settings.syncTitles || !this.canWrite(threadId)) return;
        const id = cardId({ hostId: connection.hostId, threadId });
        const record = this.records[id];
        const key = this.titleKey(id);
        if (!record?.title || this.titleInFlight.has(key) || this.editing.has(id)) return;
        this.titleInFlight.add(key);
        try {
            await this.resolveTitle(connection, threadId, id, record, record.title);
        } catch (error) {
            this.ports.proof("board-title-settlement-failed", {
                hostId: connection.hostId,
                threadId,
                error: errorText(error),
            });
        } finally {
            this.titleInFlight.delete(key);
        }
    }

    private async resolveTitle(
        connection: HostConnection,
        threadId: ThreadId,
        id: string,
        record: CardRecord,
        title: NonNullable<CardRecord["title"]>,
    ): Promise<void> {
        const account = accountKey(this.requiredContext());
        const deadline = performance.now() + titleRpcBudgetMs;
        let timer: number | undefined;
        let updated: Thread | undefined;
        let failure: string | undefined;
        try {
            const timeout = new Promise<never>((_, reject) => {
                timer = window.setTimeout(
                    () => reject(new Error("Codex 标题同步超时")),
                    titleRpcBudgetMs,
                );
            });
            updated = await Promise.race([
                this.syncTitle(connection, threadId, id, record, title.state, deadline),
                timeout,
            ]);
            if (!updated) return;
            this.desktop.assertCurrent(connection);
        } catch (error) {
            failure = errorText(error);
        } finally {
            window.clearTimeout(timer);
        }
        if (this.records[id] !== record) return;
        if (failure)
            this.nextTitleAttempt.set(id, {
                version: record.version,
                at: Date.now() + titleRetryDelayMs,
            });
        else this.nextTitleAttempt.delete(id);
        if (failure === undefined || failure !== title.error) {
            const changed = await this.ports.settleTitle({
                account,
                id,
                expected: record.version,
                ...(failure === undefined ? {} : { failure }),
            });
            if (!changed || this.records[id] !== record) return;
            const { title: _pending, ...rest } = record;
            this.records[id] = failure ? { ...rest, title: { ...title, error: failure } } : rest;
        }
        if (updated && !failure && !this.editing.has(id))
            this.publishThread(connection.hostId, updated);
    }

    private async moveCard(
        state: BoardState | null,
        connection: HostConnection,
        context: SidebarContext,
        before: Thread,
    ): Promise<Thread> {
        const source = placement(context, connection.hostId, before);
        if (presentState(context, source, this.mode) === state) return before;
        const destination = state && sectionFor(context, state);
        if (!destination || !Object.hasOwn(visibleStates(this.mode), state ?? "")) {
            throw new Error("当前 Codex 侧栏缺少唯一的目标状态分区");
        }
        let failure: unknown;
        try {
            await this.desktop.move(
                connection,
                { accountId: context.accountId, userId: context.userId },
                before.id,
                source.sectionId,
                destination,
            );
        } catch (error) {
            failure = error;
        }
        const after = await this.desktop.read(connection, before.id, false);
        const fresh = await this.freshContext();
        const expectedReplica = fresh.sections.find(({ id }) => id === destination)
            ?.hostSectionIds?.[connection.hostId];
        const confirmed = expectedReplica !== undefined && after.section?.id === expectedReplica;
        this.ports.proof("board-native-move", {
            hostId: connection.hostId,
            threadId: before.id,
            from: before.section?.id ?? null,
            to: expectedReplica ?? null,
            readBack: after.section?.id ?? null,
            confirmed,
        });
        if (!confirmed) throw new Error(failure ? errorText(failure) : "Codex 尚未确认原生位置");
        this.context = fresh;
        return after;
    }

    private async open(id: string): Promise<{ ok: true }> {
        const { card, connection } = this.target(id);
        if (!card.canOpen) throw new Error("会话目前只显示缓存，请重新发现");
        const { accountId, userId } = await this.freshContext();
        await this.desktop.read(connection, card.threadId, false);
        await this.freshContext();
        await this.desktop.navigate({
            kind: "open",
            accountId,
            userId,
            hostId: card.hostId,
            threadId: card.threadId,
        });
        this.ports.closePage();
        return { ok: true };
    }

    private async preview(id: string) {
        const { card, connection } = this.target(id);
        if (!card.canOpen) throw new Error("会话目前只显示缓存，请重新发现");
        await this.freshContext();
        const result = await previewReply(this.desktop, connection, card.threadId);
        await this.freshContext();
        this.desktop.assertCurrent(connection);
        return result;
    }

    private findProject(id: string) {
        for (const [hostId, host] of this.hosts) {
            for (const project of host.projects.values()) {
                if (projectKey(hostId, project.id) !== id) continue;
                if (!host.projectRead || !this.isLive(host))
                    throw new Error("项目当前仅有缓存，请重新发现");
                return { hostId, project };
            }
        }
        throw new Error("项目已变更，请刷新后选择");
    }

    private persistDraft(record: DraftRecord): Promise<DraftRecord> {
        return this.archive.change((old) => ({ next: stageDraft(old, record), result: record }));
    }

    private resetDraft(input: DraftInput) {
        const draft = unresolvedDrafts(this.ledger).find(
            ({ project, requestId }) =>
                project === input.projectId && requestId === input.requestId,
        );
        if (!input.confirmed || !draft) throw new Error("请确认并核对待恢复的新建请求");
        return this.persistDraft({ ...draft, stage: "abandoned" }).then(() => ({ ok: true }));
    }

    private async resumeDraft(input: DraftInput, draft: DraftRecord) {
        const requestId = draft.requestId;
        const pending = { ok: false, pending: true, requestId };
        if (input.requestId !== requestId || !draft.threadId) {
            return { ...pending, error: "请先在 Codex 核对这个旧新建请求的结果" };
        }
        const { hostId } = this.findProject(draft.project);
        const card = this.snapshot().sessions.find(
            (row) => row.hostId === hostId && row.threadId === draft.threadId,
        );
        if (!card) return { ...pending, error: "原会话尚未读回，请核对连接后刷新" };
        await this.open(card.id);
        await this.persistDraft({ ...draft, stage: "opened" });
        return { ok: true, requestId };
    }

    private async launchDraft(input: DraftInput, record?: DraftRecord) {
        const { projectId } = input;
        const { accountId, userId } = await this.freshContext();
        const selected = projectId ? this.findProject(projectId) : null;
        const host =
            selected?.hostId ?? this.desktop.knownHosts().find((hostId) => hostId === "local");
        if (!host || (input.hostId && input.hostId !== host))
            throw new Error("选定的 Codex 连接不可用");
        const connection = this.desktop.capture(host);
        const result = await this.desktop.navigate({
            kind: "new",
            accountId,
            userId,
            hostId: host,
            projectId: selected?.project.id ?? null,
            roots: selected?.project.roots.map(({ path }) => path) ?? [],
        });
        this.desktop.assertCurrent(connection);
        if (result !== "registration" && result !== "composer")
            throw new Error("Codex 未确认输入页");
        if (record && result === "composer")
            await this.persistDraft({ ...record, stage: "opened" });
        this.ports.closePage();
        return {
            ok: true,
            registration: result === "registration",
            ...(input.operationId ? { operationId: input.operationId } : {}),
        };
    }

    private async create(input: DraftInput) {
        const { projectId, operationId } = input;
        const draft = unresolvedDrafts(this.ledger).find(
            ({ project, source }) => project === projectId && source === "legacy",
        );
        if (draft) return this.resumeDraft(input, draft);
        if (input.requestId) throw new Error("旧新建请求已结束，请重新选择目标");
        if (operationId && (!projectId || !/^[0-9a-f-]{36}$/i.test(operationId)))
            throw new Error("新建操作 ID 无效");
        const existing = this.ledger.pendingDrafts?.find(
            ({ requestId }) => requestId === operationId,
        );
        if (existing?.project !== undefined && existing.project !== projectId)
            throw new Error("新建操作已用于其他项目");
        if (existing?.stage === "opened") return { ok: true, operationId };
        const record =
            projectId && operationId
                ? await this.persistDraft({
                      project: projectId,
                      requestId: operationId,
                      stage: "starting",
                      threadId: null,
                      source: "native",
                  })
                : undefined;
        try {
            return await this.launchDraft(input, record);
        } catch (error) {
            if (!record) throw error;
            return { ok: false, pending: true, operationId, error: errorText(error) };
        }
    }

    private async assign(id: string, projectId: string): Promise<{ ok: true }> {
        const { card, connection } = this.target(id);
        const { hostId, project } = this.findProject(projectId);
        if (
            hostId !== card.hostId ||
            card.readOnly ||
            project.roots.length !== 1 ||
            project.roots[0]?.path !== card.cwd
        ) {
            throw new Error("目标项目与会话主机或工作目录不一致");
        }
        await this.freshContext();
        const before = await this.desktop.read(connection, card.threadId, false);
        if (
            before.cwd !== card.cwd ||
            (before.projectId !== null && before.projectId !== project.id)
        )
            throw new Error("项目归属已变更，请刷新后重试");
        await this.freshContext();
        let failure: unknown;
        if (before.projectId !== project.id) {
            try {
                await this.desktop.assignProject(connection, card.threadId, project.id);
            } catch (error) {
                failure = error;
            }
        }
        const actual =
            before.projectId === project.id
                ? before
                : await this.desktop.read(connection, card.threadId, false);
        await this.freshContext();
        this.desktop.assertCurrent(connection);
        if (actual.projectId !== project.id || actual.cwd !== card.cwd)
            throw new Error(failure ? errorText(failure) : "Codex 尚未确认项目归属");
        this.publishThread(hostId, actual);
        return { ok: true };
    }

    private async writeLedger(next: BoardLedger): Promise<void> {
        const context = await this.freshContext();
        await this.ports.writeLedger(accountKey(context), next);
        this.ledger = next;
    }

    private sort(state: string, requested: SortPreference) {
        if (
            !Object.hasOwn(visibleStates(this.mode), state) ||
            requested.field !== "updated_at" ||
            !["asc", "desc"].includes(requested.direction)
        )
            throw new Error("排序不合法");
        return this.archive.change((old) => {
            const current = old.columnSorts[state];
            if (requested.version !== (current?.version ?? 0))
                throw new Error("排序已变更，请刷新");
            const preference = {
                field: requested.field,
                direction: requested.direction,
                version: requested.version + 1,
            };
            return {
                next: { ...old, columnSorts: { ...old.columnSorts, [state]: preference } },
                result: { ok: true, state, preference },
            };
        });
    }

    private async verifyArchive(item: ArchiveItem) {
        const { card, connection } = this.target(item.id);
        if (!card.canArchive || card.state !== "to_archive" || this.editing.has(item.id)) {
            throw new Error("当前状态不能归档");
        }
        const context = await this.freshContext();
        const before = await this.desktop.read(connection, card.threadId, false);
        const latest = (await this.desktop.turns(connection, card.threadId, "summary")).data[0];
        const position = placement(context, card.hostId, before);
        const record = this.records[item.id] ?? emptyRecord;
        const input = turnInput(latest);
        if (
            (record.confirmedInputKey && input !== record.confirmedInputKey) ||
            changedInput(record, input) ||
            item.inputVersion !== record.inputVersion ||
            before.status.type === "active" ||
            latest?.status === "inProgress"
        ) {
            throw new NewInputError("出现新输入或仍在执行，请重新核对");
        }
        if (
            item.version !== record.version ||
            item.sectionVersion !== position.version ||
            !position.managed ||
            presentState(context, position, this.mode) !== "to_archive"
        ) {
            throw new Error("归档状态或位置已变更");
        }
        const after = await this.desktop.read(connection, card.threadId, false);
        if (
            after.status.type !== before.status.type ||
            placement(context, card.hostId, after).version !== position.version
        ) {
            throw new Error("归档核对期间会话发生了变化");
        }
        return { connection, threadId: after.id };
    }

    private async archiveThread(item: ArchiveItem): Promise<void> {
        const { connection, threadId } = await this.verifyArchive(item).catch((error: unknown) => {
            if (error instanceof NewInputError) throw error;
            throw new ArchivePreconditionError(errorText(error));
        });
        await this.desktop.setArchived(connection, threadId, true);
        await this.freshContext();
    }

    private async archived(id: string): Promise<boolean> {
        const known = [...this.hosts].find(([hostId, host]) =>
            [...host.active.values(), ...host.archived.values()].some(
                (thread) => cardId({ hostId, threadId: thread.id }) === id,
            ),
        );
        if (!known) return false;
        const [hostId, host] = known;
        const connection = this.desktop.capture(hostId);
        const archived = new Map<ThreadId, Thread>();
        await this.readPages(connection, true, archived);
        await this.freshContext();
        this.desktop.assertCurrent(connection);
        const result = [...archived.values()].find(
            (thread) => cardId({ hostId, threadId: thread.id }) === id,
        );
        if (result) host.currentActive.delete(result.id);
        this.mergeArchives(host, archived);
        return result !== undefined;
    }

    private autoArchive(context: SidebarContext): void {
        if (this.mode === "legacy") return;
        const now = Date.now() / 1000;
        for (const [hostId, host] of this.hosts) {
            const seconds = this.archiveInterval(hostId);
            if (!seconds || !this.isLive(host)) continue;
            const next = this.archiveDeadlines.get(hostId) ?? now + seconds;
            this.archiveDeadlines.set(hostId, next);
            if (next > now) continue;
            this.archiveDeadlines.set(hostId, now + seconds);
            void this.autoArchiveHost(hostId, context);
        }
    }

    private async autoArchiveHost(hostId: HostId, context: SidebarContext): Promise<void> {
        const items = this.snapshot().sessions.filter(
            (card) =>
                card.hostId === hostId &&
                card.state === "to_archive" &&
                card.canArchive &&
                !card.archivePending &&
                !card.followUp,
        );
        const run: AutoArchiveRun = { at: Date.now() / 1000, archived: [], deferred: [] };
        if (!items.length) {
            this.archiveRuns.set(hostId, run);
            return;
        }
        const id = crypto.randomUUID();
        try {
            await this.archive.start(
                id,
                items.map(({ id, title, version, inputVersion, sectionVersion }) => ({
                    id,
                    title,
                    version,
                    inputVersion,
                    sectionVersion,
                })),
            );
            for (const item of items) {
                if (accountKey(context) !== accountKey(this.requiredContext())) return;
                const result = await this.archive.submit(id, item);
                if (result.status === "archived" || result.status === "verified")
                    run.archived.push(item.id);
                else run.deferred.push({ id: item.id, reason: result.message ?? result.status });
            }
            await this.archive.finish(id);
        } catch (error) {
            run.error = errorText(error);
        }
        if (accountKey(context) === accountKey(this.requiredContext()))
            this.archiveRuns.set(hostId, run);
    }

    private async requestArchive(path: string, body: object | undefined) {
        await this.freshContext();
        const args = body as
            | (Partial<ArchiveTarget> & { batchId?: string; items?: ArchiveTarget[] })
            | undefined;
        if (path === "/api/archive-batches" && !body) return { batches: this.archive.batches() };
        if (this.mode === "legacy") throw new Error("旧六状态模式不支持归档");
        if (!args?.id) throw new Error("缺少归档批次或会话 ID");
        if (path === "/api/archive-batches" && Array.isArray(args.items))
            return { batch: await this.archive.start(args.id, args.items) };
        if (path === "/api/archive-batches/finish")
            return { batch: await this.archive.finish(args.id) };
        if (path === "/api/archive-batches/check")
            return { batch: await this.archive.check(args.id) };
        if (path === "/api/archive") {
            const candidate = {
                ...(args as ArchiveTarget),
                sectionVersion: args.sectionVersion ?? null,
            };
            if (args.batchId) return { item: await this.archive.submit(args.batchId, candidate) };
            const { card } = this.target(candidate.id);
            return { item: await this.archive.single({ ...candidate, title: card.title }) };
        }
        throw new Error("归档请求不完整");
    }

    async request(path: string, body?: object): Promise<unknown> {
        if (path === "/api/sessions" || path === "/api/refresh") {
            await this.refresh();
            return this.snapshot();
        }
        if (path === "/api/state" && body) return this.update(body as Edit);
        if (path.startsWith("/api/archive")) return this.requestArchive(path, body);
        if (path === "/api/column-sort" && body) {
            const sort = body as SortPreference & { state: string };
            return this.sort(sort.state, sort);
        }
        const input = body as (DraftInput & { id?: string }) | undefined;
        if (path === "/api/open" && input?.id) return this.open(input.id);
        if (path === "/api/preview-reply" && input?.id) return this.preview(input.id);
        if (path === "/api/new") return this.create(input ?? {});
        if (path === "/api/new/reset" && input) return this.resetDraft(input);
        if (path === "/api/project/assign" && input?.confirmed && input.id && input.projectId)
            return this.assign(input.id, input.projectId);
        if (["/api/connect", "/api/ssh-setup"].includes(path)) {
            const { accountId, userId } = await this.freshContext();
            await this.desktop.navigate({ kind: "settings", accountId, userId });
            this.ports.closePage();
            if (path === "/api/ssh-setup")
                return { ok: true, state: "applied", message: "已打开 Codex 远程连接设置" };
            return { ok: true };
        }
        if (path === "/api/plugin-update") return this.ports.updateStatus(body !== undefined);
        throw new Error("当前看板尚未接入这项操作");
    }
}
