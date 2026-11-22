import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { type BoardLedger, emptyLedger } from "./archive.js";
import type { DraftRecord } from "./drafts.js";
import type { BoardState } from "./placement.js";

export interface CardRecord {
    note: string;
    version: number;
    inputVersion: number;
    inputKey?: string;
    confirmedInputKey?: string;
    manual?: boolean;
    phaseKey?: string;
    previewTurnId?: string;
    selectedAt?: number;
    reason?: string;
    title?: { state: BoardState | null; error?: string };
}

interface RecordRevision {
    account: string;
    id: string;
    expected: number;
}

export interface RecordWrite extends RecordRevision {
    record: CardRecord;
}

export interface TitleResolution extends RecordRevision {
    failure?: string;
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

    private read(): Record<string, Record<string, CardRecord>> {
        return readRecord<Record<string, CardRecord>>(this.file);
    }

    account(key: string, imported: Record<string, CardRecord> = {}): Record<string, CardRecord> {
        const accounts = this.read();
        return { ...imported, ...(Object.hasOwn(accounts, key) ? accounts[key] : undefined) };
    }

    write(update: RecordWrite, imported: Record<string, CardRecord> = {}): void {
        if (!this.change(update, imported, () => update.record))
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

    private change(
        { account, id, expected }: RecordRevision,
        imported: Record<string, CardRecord>,
        apply: (current: CardRecord | undefined) => CardRecord | undefined,
    ): boolean {
        const accounts = this.read();
        const rows = Object.hasOwn(accounts, account) ? (accounts[account] ?? {}) : {};
        const current = Object.hasOwn(rows, id)
            ? rows[id]
            : Object.hasOwn(imported, id)
              ? imported[id]
              : undefined;
        if ((current?.version ?? 0) !== expected) return false;
        const record = apply(current);
        if (!record) return false;
        atomicWrite(this.file, { ...accounts, [account]: { ...rows, [id]: record } });
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
