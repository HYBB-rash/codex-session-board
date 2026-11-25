import { createHash } from "node:crypto";
import { join } from "node:path";
import { DatabaseSync, type SQLOutputValue } from "node:sqlite";
import { hostId } from "../extensions/desktop/connection.js";
import type { CardRecord } from "../extensions/records.js";
import { databaseName } from "./database.js";
import { readRegular } from "./files.js";
import { type LegacyMigration, readLegacyMigration, verifyLegacyMigration } from "./legacy.js";

type OldColumn =
    | "id"
    | "title"
    | "cwd"
    | "state"
    | "note"
    | "changed_at"
    | "source_sequence"
    | "version"
    | "input_version"
    | "input_revision"
    | "selection_turn"
    | "selection_users"
    | "selected_at"
    | "selected_state"
    | "automatic_state"
    | "automatic_reason"
    | "native_state"
    | "native_section"
    | "native_revision"
    | "runtime_key"
    | "runtime_pending_key"
    | "needs_review"
    | "archived"
    | "project_id"
    | "key"
    | "value"
    | "host_id"
    | "thread_id"
    | "updated_at"
    | "turn_json"
    | "observed_revision"
    | "observed_at"
    | "batch_id"
    | "created_at"
    | "finished_at"
    | "position"
    | "target"
    | "status"
    | "reason"
    | "source"
    | "checked_at"
    | "requested_at"
    | "attempted_at"
    | "error";

export type OldRow = Record<string, SQLOutputValue> & Partial<Record<OldColumn, SQLOutputValue>>;
const states = ["unknown", "in_progress", "review", "blocked", "to_archive", "completed"] as const;
type LegacyColumn = (typeof states)[number];

export interface LegacyAccountBinding {
    accountId: string;
    userId: string;
    hosts: Readonly<Record<string, string>>;
}

export interface LegacyCardRecord extends CardRecord {
    sourceHostId: string;
    hostId: string;
    threadId: string;
    displayTitle: string;
    cwd: string;
    updatedAt: number;
    followUp: boolean;
    archived: boolean;
    selectedState: LegacyColumn | null;
    selectionTurn: string | null;
    selectionUsers: string[];
    runtimeKey: string | null;
    runtimePendingKey: string | null;
    automaticState: LegacyColumn | null;
    nativeState: LegacyColumn | null;
    nativeSection: string | null;
    oldProjectId: string | null;
}

interface LegacyTurn {
    id?: unknown;
    userItems?: unknown;
    hasFinal?: unknown;
}

interface ColumnSort {
    field: "updated_at";
    direction: "asc" | "desc";
    version: number;
}

export interface LegacyPreferences {
    mode: "four" | "five" | "legacy";
    syncTitles: boolean;
    nativeSections: boolean;
    autoArchiveSeconds: number | null;
    remoteAutoArchiveSeconds: number | null;
    pollSeconds: number;
    runtimePollSeconds: number;
    pageSize: number;
    maxPages: number;
    historyPerPoll: number;
}

export interface LegacyImportDatabase {
    sourceHostId: string;
    hostId: string;
    settings: Record<string, string>;
    projects: OldRow[];
    titleUpdates: OldRow[];
    archiveBatches: OldRow[];
    archiveItems: OldRow[];
}

export interface LegacyCardChange {
    changedAt: number;
    state: string | null;
    note: string;
    source: { database: string; sequence: number };
}

export interface MigratedBoard {
    migrationId: string;
    accountKey: string;
    preferences: LegacyPreferences;
    cards: Record<string, LegacyCardRecord>;
    changes: Record<string, LegacyCardChange[]>;
    columnSorts: Partial<Record<LegacyColumn, ColumnSort>>;
    databases: LegacyImportDatabase[];
    unboundHostDirectories: string[];
}

interface OldConfig {
    host_id?: unknown;
    legacy_mode?: unknown;
    four_columns?: unknown;
    native_sections?: unknown;
    sync_titles?: unknown;
    auto_archive_seconds?: unknown;
    remote_auto_archive_seconds?: unknown;
    poll_seconds?: unknown;
    runtime_poll_seconds?: unknown;
    page_size?: unknown;
    max_pages?: unknown;
    history_per_poll?: unknown;
}

interface Scope {
    sourceHostId: string;
    hostId: string;
}

function validInteger(value: unknown): value is number {
    return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function count(value: unknown): number {
    return validInteger(value) ? value : 0;
}

function positive(value: unknown): number | null {
    return validInteger(value) && value > 0 ? value : null;
}

function finite(value: unknown): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function text(value: unknown): string | null {
    return typeof value === "string" ? value : null;
}

function state(value: unknown): LegacyColumn | null {
    return states.find((label) => label === value) ?? null;
}

function json(value: unknown): unknown {
    return typeof value === "string" ? JSON.parse(value) : null;
}

function userItems(value: unknown): string[] {
    const items = json(value);
    return Array.isArray(items)
        ? items.filter((item): item is string => typeof item === "string")
        : [];
}

function turnKey(id: unknown, items: unknown): string | undefined {
    return typeof id === "string" &&
        id &&
        Array.isArray(items) &&
        items.every((item) => typeof item === "string")
        ? JSON.stringify([id, [...items].sort()])
        : undefined;
}

function cardEvidence(
    row: OldRow,
): Pick<CardRecord, "inputKey" | "confirmedInputKey" | "previewTurnId" | "selectedAt"> {
    const turn = json(row.turn_json) as LegacyTurn | null;
    const inputKey = turnKey(turn?.id, turn?.userItems);
    const selectedAt = finite(row.selected_at);
    const confirmedInputKey = turnKey(row.selection_turn, json(row.selection_users));
    return {
        ...(inputKey ? { inputKey } : {}),
        ...(confirmedInputKey ? { confirmedInputKey } : {}),
        ...(turn?.hasFinal === true && typeof turn.id === "string"
            ? { previewTurnId: turn.id }
            : {}),
        ...(selectedAt === null ? {} : { selectedAt }),
    };
}

function readPreferences(config: OldConfig): LegacyPreferences {
    return {
        mode:
            config.legacy_mode === true ? "legacy" : config.four_columns === true ? "four" : "five",
        nativeSections: config.native_sections === true,
        syncTitles: config.sync_titles !== false,
        autoArchiveSeconds: positive(config.auto_archive_seconds),
        remoteAutoArchiveSeconds: positive(config.remote_auto_archive_seconds),
        pollSeconds: positive(config.poll_seconds) ?? 30,
        runtimePollSeconds: positive(config.runtime_poll_seconds) ?? 2,
        pageSize: positive(config.page_size) ?? 100,
        maxPages: positive(config.max_pages) ?? 100,
        historyPerPoll: positive(config.history_per_poll) ?? 20,
    };
}

const tables = [
    "sessions",
    "remote_sessions",
    "changes",
    "settings",
    "remote_projects",
    "title_updates",
    "archive_batches",
    "archive_batch_items",
] as const;
type OldTable = (typeof tables)[number];

class OldDatabase {
    constructor(
        private readonly database: DatabaseSync,
        private readonly relative: string,
    ) {}

    rows(table: OldTable): OldRow[] {
        const found = this.database
            .prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?")
            .get(table);
        if (!found) return [];
        const select =
            table === "changes"
                ? "rowid AS source_sequence, * FROM changes ORDER BY rowid"
                : `* FROM ${table}`;
        return this.database.prepare(`SELECT ${select}`).all();
    }

    changeHistory(): Map<string, LegacyCardChange[]> {
        const history = new Map<string, LegacyCardChange[]>();
        for (const row of this.rows("changes")) {
            const id = text(row.thread_id);
            const changedAt = finite(row.changed_at);
            const note = text(row.note);
            const sequence = row.source_sequence;
            if (!id || changedAt === null || note === null || !validInteger(sequence)) continue;
            const changes = history.get(id) ?? [];
            changes.push({
                changedAt,
                state: text(row.state),
                note,
                source: { database: this.relative, sequence },
            });
            history.set(id, changes);
        }
        return history;
    }
}

function representCard(
    scope: Scope,
    row: OldRow,
    threadId: string,
    remoteCache: boolean,
): LegacyCardRecord {
    const savedState = state(remoteCache ? row.state : row.selected_state);
    const manual = remoteCache ? count(row.version) > 0 : savedState !== null;
    const reason = text(row.automatic_reason);
    const phaseKey = text(row.runtime_key);
    const pendingPhaseKey = text(row.runtime_pending_key);
    return {
        ...scope,
        ...cardEvidence(row),
        ...(reason ? { reason } : {}),
        ...(phaseKey ? { phaseKey } : {}),
        ...(pendingPhaseKey ? { pendingPhaseKey } : {}),
        threadId,
        displayTitle: text(row.title) ?? "",
        cwd: text(row.cwd) ?? "",
        updatedAt: count(row.updated_at),
        note: text(row.note) ?? "",
        version: count(row.version),
        inputVersion: count(remoteCache ? row.input_version : row.input_revision),
        manual,
        selectedState: manual ? savedState : null,
        selectionTurn: text(row.selection_turn),
        selectionUsers: userItems(row.selection_users),
        automaticState: state(row.automatic_state),
        nativeState: state(row.native_state),
        nativeSection: text(row.native_section ?? row.native_revision),
        runtimeKey: text(row.runtime_key),
        runtimePendingKey: text(row.runtime_pending_key),
        followUp: row.needs_review === 1,
        archived: row.archived === 1,
        oldProjectId: text(row.project_id),
    };
}

function scopeFor(
    bindings: LegacyAccountBinding["hosts"],
    sourceHostId: string,
): Scope | undefined {
    const target = Object.hasOwn(bindings, sourceHostId) ? bindings[sourceHostId] : undefined;
    return target ? { sourceHostId, hostId: target } : undefined;
}

function remoteSidecar(migration: LegacyMigration, directory: string): string | undefined {
    const file = `remote-hosts/${directory}/source.json`;
    if (!migration.receipt.files.some(({ name }) => name === file)) return undefined;
    try {
        const source = JSON.parse(
            readRegular(join(migration.directory, file)).toString("utf8"),
        ) as { hostId?: unknown };
        const host = text(source?.hostId);
        if (host && createHash("sha256").update(host).digest("hex") === directory) return host;
    } catch (error) {
        if (!(error instanceof SyntaxError)) throw error;
    }
    return undefined;
}

function oldCardId(record: LegacyCardRecord): string {
    return record.hostId === "local"
        ? record.threadId
        : JSON.stringify([record.hostId, record.threadId]);
}

function addCard(
    board: MigratedBoard,
    record: LegacyCardRecord,
    history: LegacyCardChange[] | undefined,
): void {
    const id = oldCardId(record);
    board.cards[id] = record;
    if (!history?.length) return;
    const existing = board.changes[id];
    board.changes[id] = existing ? [...existing, ...history].sort(changeOrder) : history;
}

function changeOrder(left: LegacyCardChange, right: LegacyCardChange): number {
    const leftDatabase = left.source.database;
    const rightDatabase = right.source.database;
    if (leftDatabase === rightDatabase) return left.source.sequence - right.source.sequence;
    if (leftDatabase === databaseName) return -1;
    if (rightDatabase === databaseName) return 1;
    return leftDatabase.localeCompare(rightDatabase);
}

function readSettings(database: OldDatabase): Record<string, string> {
    return Object.fromEntries(
        database.rows("settings").flatMap((row) => {
            const key = text(row.key);
            const value = text(row.value);
            return key === null || value === null ? [] : [[key, value]];
        }),
    );
}

function readSorts(settings: Record<string, string>): MigratedBoard["columnSorts"] {
    const entries = states.flatMap((label) => {
        const value = json(settings[`column_sort:${label}`]) as Partial<ColumnSort> | null;
        return value?.field === "updated_at" &&
            (value.direction === "asc" || value.direction === "desc") &&
            validInteger(value.version)
            ? [[label, value]]
            : [];
    });
    return Object.fromEntries(entries);
}

function readDatabase(
    migration: LegacyMigration,
    relative: string,
    action: (db: OldDatabase) => void,
): void {
    if (!migration.receipt.files.some(({ name }) => name === relative)) return;
    const database = new DatabaseSync(join(migration.directory, relative), { readOnly: true });
    try {
        database.exec("PRAGMA trusted_schema=OFF");
        action(new OldDatabase(database, relative));
    } finally {
        database.close();
    }
}

function readBoundDatabase(
    database: OldDatabase,
    scope: Scope,
    board: MigratedBoard,
    history = database.changeHistory(),
) {
    const settings = readSettings(database);
    const titleUpdates = database.rows("title_updates");
    const titleJobs = new Map(
        titleUpdates.flatMap((row) =>
            typeof row.thread_id === "string" ? ([[row.thread_id, row]] as const) : [],
        ),
    );
    board.databases.push({
        ...scope,
        settings,
        projects: database.rows("remote_projects"),
        titleUpdates,
        archiveBatches: database.rows("archive_batches"),
        archiveItems: database.rows("archive_batch_items"),
    });
    for (const row of database.rows("sessions")) {
        const id = text(row.id);
        if (!id) throw new Error("旧会话没有会话 ID");
        const record = representCard(scope, row, id, false);
        const job = titleJobs.get(id);
        const jobState = state(job?.state);
        if (jobState && job?.version === record.version) {
            const error = text(job.error);
            record.title = { state: jobState, ...(error ? { error } : {}) };
        }
        addCard(board, record, history.get(id));
    }
    return settings;
}

function readRootDatabase(
    database: OldDatabase,
    config: OldConfig,
    binding: LegacyAccountBinding,
    board: MigratedBoard,
) {
    const scope = scopeFor(binding.hosts, text(config.host_id) ?? "local");
    const history = database.changeHistory();
    if (scope) board.columnSorts = readSorts(readBoundDatabase(database, scope, board, history));
    for (const row of database.rows("remote_sessions")) {
        const source = text(row.host_id);
        const id = text(row.thread_id);
        const remote = source && scopeFor(binding.hosts, source);
        if (!id || !remote) continue;
        const record = representCard(remote, row, id, true);
        addCard(board, record, history.get(text(row.id) ?? ""));
    }
}

export function assertBinding(binding: LegacyAccountBinding): void {
    if (!binding.accountId.trim() || !binding.userId.trim())
        throw new Error("迁移需要明确的 Codex 账号绑定");
    const destinations = new Set<string>();
    for (const [source, destination] of Object.entries(binding.hosts)) {
        if (
            !source.trim() ||
            !hostId(destination) ||
            destinations.has(destination) ||
            (source === "local") !== (destination === "local")
        ) {
            throw new Error("迁移主机绑定必须一对一，本机只允许绑定本机");
        }
        destinations.add(destination);
    }
}

export function readMigratedBoard(
    destination: string,
    binding: LegacyAccountBinding,
): MigratedBoard {
    assertBinding(binding);
    const migration = readLegacyMigration(destination);
    verifyLegacyMigration(migration);
    const config = JSON.parse(
        readRegular(join(migration.directory, "config.json")).toString("utf8"),
    ) as OldConfig;
    const board: MigratedBoard = {
        migrationId: migration.receipt.id,
        accountKey: JSON.stringify([binding.accountId, binding.userId]),
        preferences: readPreferences(config),
        cards: Object.create(null),
        changes: Object.create(null),
        columnSorts: {},
        databases: [],
        unboundHostDirectories: [],
    };
    readDatabase(migration, databaseName, (database) =>
        readRootDatabase(database, config, binding, board),
    );
    for (const directory of migration.receipt.remoteHosts) {
        const source = remoteSidecar(migration, directory);
        const scope = source && scopeFor(binding.hosts, source);
        if (!scope) {
            board.unboundHostDirectories.push(directory);
            continue;
        }
        readDatabase(migration, `remote-hosts/${directory}/${databaseName}`, (database) => {
            readBoundDatabase(database, scope, board);
        });
    }
    return board;
}
