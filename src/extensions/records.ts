import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { ThreadRef } from "./desktop/connection.js";
import { type BoardState, labels } from "./placement.js";
import { type BoardLedger, emptyLedger } from "./session-board/archive.js";
import type { DraftRecord } from "./session-board/drafts.js";

export interface CardRecord {
    note: string;
    version: number;
    inputVersion: number;
    inputKey?: string;
    confirmedInputKey?: string;
    manual?: boolean;
    phaseKey?: string;
    pendingPhaseKey?: string;
    previewTurnId?: string;
    selectedAt?: number;
    reason?: string;
    title?: { state: BoardState | null; error?: string };
}

export interface LegacyCachedCard extends ThreadRef {
    title: string;
    cwd: string;
    updatedAt: number;
    state: BoardState | null;
    archived: boolean;
    followUp: boolean;
}

export interface LoadedBoardRecords {
    records: Record<string, CardRecord>;
    legacyCachedCards: LegacyCachedCard[];
}

interface RecordRevision {
    account: string;
    id: string;
    expected: number;
}

export interface RecordWrite extends RecordRevision {
    record: CardRecord;
    change?: RecordChange;
}

export type RecordChange =
    | { kind: "manual"; state: BoardState | null }
    | { kind: "automatic"; state: BoardState };

type SavedChange = RecordChange & {
    changedAt: number;
    note: string;
    version: number;
};

interface StoredCardRecord extends CardRecord {
    changes?: SavedChange[];
}

export interface TitleResolution extends RecordRevision {
    failure?: string;
}

export interface PhaseIntent extends RecordRevision {
    phaseKey: string;
}

function readRecord<T>(file: string): Record<string, T> {
    try {
        return JSON.parse(readFileSync(file, "utf8")) as Record<string, T>;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
        return {};
    }
}

function mergeDrafts(imported: DraftRecord[] = [], saved: DraftRecord[] = []): DraftRecord[] {
    return [...new Map([...imported, ...saved].map((draft) => [draftKey(draft), draft])).values()];
}

function validRecordChange(value: unknown, record: CardRecord): boolean {
    if (value === undefined) return true;
    if (!value || typeof value !== "object" || !("state" in value) || !("kind" in value))
        return false;
    const { kind, state } = value;
    const column = typeof state === "string" && Object.hasOwn(labels, state);
    if (kind === "manual") return record.manual === true && (column || state === null);
    return (
        kind === "automatic" &&
        record.manual === false &&
        column &&
        typeof record.reason === "string"
    );
}

function savedChange(change: RecordChange, record: CardRecord): SavedChange {
    const metadata = { changedAt: Date.now() / 1000, version: record.version };
    const { kind, state } = change;
    return kind === "manual"
        ? { ...metadata, kind, state, note: record.note }
        : { ...metadata, kind, state, note: record.reason ?? "" };
}

function draftKey({ project, requestId }: DraftRecord): string {
    return JSON.stringify([project, requestId]);
}

function changedLedger(ledger: BoardLedger, imported: BoardLedger): BoardLedger {
    const seedBatches = new Map(imported.batches.map((batch) => [batch.id, batch]));
    const seedDrafts = new Map(imported.pendingDrafts?.map((draft) => [draftKey(draft), draft]));
    return {
        batches: ledger.batches.filter(
            (batch) => !isDeepStrictEqual(seedBatches.get(batch.id), batch),
        ),
        columnSorts: Object.fromEntries(
            Object.entries(ledger.columnSorts).filter(
                ([state, sort]) => !isDeepStrictEqual(imported.columnSorts[state], sort),
            ),
        ),
        ...(ledger.pendingDrafts && {
            pendingDrafts: ledger.pendingDrafts.filter(
                (draft) => !isDeepStrictEqual(seedDrafts.get(draftKey(draft)), draft),
            ),
        }),
    };
}

export class BoardLedgers {
    constructor(private readonly file: string) {}

    load(account: string, imported = emptyLedger()): BoardLedger {
        const saved = readRecord<BoardLedger>(this.file)[account];
        if (!saved) return imported;
        const originals = new Map(imported.batches.map((batch) => [batch.id, batch]));
        for (const batch of saved.batches) {
            const items = new Map(originals.get(batch.id)?.items.map((item) => [item.id, item]));
            for (const item of batch.items) items.set(item.id, item);
            originals.set(batch.id, { ...batch, items: [...items.values()] });
        }
        return {
            batches: [...originals.values()].sort(
                (left, right) => right.createdAt - left.createdAt,
            ),
            columnSorts: { ...imported.columnSorts, ...saved.columnSorts },
            ...((imported.pendingDrafts || saved.pendingDrafts) && {
                pendingDrafts: mergeDrafts(imported.pendingDrafts, saved.pendingDrafts),
            }),
        };
    }

    save(account: string, ledger: BoardLedger, imported = emptyLedger()): void {
        atomicWrite(this.file, {
            ...readRecord<BoardLedger>(this.file),
            [account]: changedLedger(ledger, imported),
        });
    }
}

export class BoardRecords {
    constructor(private readonly file: string) {}

    private read(): Record<string, Record<string, StoredCardRecord>> {
        return readRecord<Record<string, StoredCardRecord>>(this.file);
    }

    account(key: string, imported: Record<string, CardRecord> = {}): Record<string, CardRecord> {
        const accounts = this.read();
        const saved = Object.hasOwn(accounts, key) ? accounts[key] : undefined;
        const cards = Object.entries(saved ?? {}).map(([id, { changes: _changes, ...record }]) => [
            id,
            record,
        ]);
        return { ...imported, ...Object.fromEntries(cards) };
    }

    write(update: RecordWrite, imported: Record<string, CardRecord> = {}): void {
        if (!validRecordChange(update.change, update.record)) throw new Error("无效的看板变更记录");
        if (!this.change(update, imported, () => update.record, update.change))
            throw new Error("卡片已更新，请刷新后重试");
    }

    settleTitle(update: TitleResolution, imported: Record<string, CardRecord> = {}): boolean {
        return this.change(update, imported, (current) => {
            if (!current?.title) return;
            const { title, ...record } = current;
            return update.failure
                ? { ...record, title: { ...title, error: update.failure } }
                : record;
        });
    }

    preparePhase(update: PhaseIntent, imported: Record<string, CardRecord> = {}): boolean {
        return this.change(update, imported, (current) => ({
            ...(current ?? { note: "", version: 0, inputVersion: 0 }),
            pendingPhaseKey: update.phaseKey,
        }));
    }

    private change(
        { account, id, expected }: RecordRevision,
        imported: Record<string, CardRecord>,
        apply: (current: CardRecord | undefined) => CardRecord | undefined,
        event?: RecordChange,
    ): boolean {
        const accounts = this.read();
        const rows = Object.hasOwn(accounts, account) ? (accounts[account] ?? {}) : {};
        const saved = Object.hasOwn(rows, id) ? rows[id] : undefined;
        const current = saved ?? (Object.hasOwn(imported, id) ? imported[id] : undefined);
        if ((current?.version ?? 0) !== expected) return false;
        const record = apply(current);
        if (!record) return false;
        const { changes: _changes, ...card } = record as StoredCardRecord;
        const changes = saved?.changes ?? [];
        if (event) changes.push(savedChange(event, card));
        const stored = changes.length ? { ...card, changes } : card;
        atomicWrite(this.file, { ...accounts, [account]: { ...rows, [id]: stored } });
        return true;
    }
}

export class TitleEpochs {
    constructor(private readonly file: string) {}

    start(account: string, imported?: number): number {
        const epochs = readRecord<number>(this.file);
        if (Object.hasOwn(epochs, account) && Number.isFinite(epochs[account]))
            return epochs[account] as number;
        const epoch = imported ?? Date.now() / 1000;
        atomicWrite(this.file, { ...epochs, [account]: epoch });
        return epoch;
    }
}

export function atomicWrite(target: string, value: unknown): void {
    mkdirSync(dirname(target), { recursive: true, mode: 0o700 });
    const file = `${target}.${randomUUID()}.tmp`;
    try {
        writeFileSync(file, JSON.stringify(value), { flag: "wx", mode: 0o600, flush: true });
        renameSync(file, target);
    } catch (error) {
        try {
            unlinkSync(file);
        } catch (cleanup) {
            if ((cleanup as NodeJS.ErrnoException).code !== "ENOENT") throw cleanup;
        }
        throw error;
    }
}
