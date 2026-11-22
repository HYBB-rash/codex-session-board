export interface ArchiveTarget {
    id: string;
    title: string;
    version: number;
    inputVersion: number;
    sectionVersion: string | null;
}

export type ArchiveState =
    | "awaiting"
    | "pending"
    | "archived"
    | "verified"
    | "newInput"
    | "rejected";
export type ArchiveItem = ArchiveTarget & { status: ArchiveState; message?: string };

export interface ArchiveBatch {
    id: string;
    createdAt: number;
    finishedAt: number | null;
    items: ArchiveItem[];
    single?: true;
}

export interface SortPreference {
    field: "updated_at";
    direction: "asc" | "desc";
    version: number;
}

export interface BoardLedger {
    batches: ArchiveBatch[];
    columnSorts: Record<string, SortPreference>;
    pendingDrafts?: DraftRecord[];
}

interface ArchivePorts {
    scope(): string;
    ledger(): BoardLedger;
    write(next: BoardLedger): Promise<void>;
    verify(item: ArchiveItem): Promise<void>;
    isArchived(id: string): Promise<boolean>;
    archive(item: ArchiveItem): Promise<void>;
}

export class ArchivePreconditionError extends Error {}
export class NewInputError extends ArchivePreconditionError {}

export function emptyLedger(): BoardLedger {
    return { batches: [], columnSorts: {} };
}

export class Archive {
    private readonly submitted = new Set<string>();
    private tail: Promise<unknown> = Promise.resolve();

    constructor(private readonly ports: ArchivePorts) {}

    batches(): ArchiveBatch[] {
        return this.ports.ledger().batches;
    }

    private enqueue<T>(work: () => Promise<T>): Promise<T> {
        const operation = this.tail.then(work);
        this.tail = operation.then(
            () => undefined,
            () => undefined,
        );
        return operation;
    }

    async change<T>(transform: (old: BoardLedger) => { next: BoardLedger; result: T }): Promise<T> {
        const scope = this.ports.scope();
        return this.enqueue(async () => {
            this.requireScope(scope);
            const { next, result } = transform(this.ports.ledger());
            await this.ports.write(next);
            this.requireScope(scope);
            return result;
        });
    }

    private requireScope(scope: string): void {
        if (scope !== this.ports.scope()) throw new Error("归档期间 Codex 账号已变更");
    }

    private requireBatch(id: string): ArchiveBatch {
        const batch = this.batches().find((entry) => entry.id === id);
        if (!batch) throw new Error("找不到归档批次");
        return batch;
    }

    start(id: string, items: ArchiveTarget[], single?: true): Promise<ArchiveBatch> {
        if (
            !/^[\da-f-]{36}$/i.test(id) ||
            items.length === 0 ||
            items.length !== new Set(items.map((item) => item.id)).size
        )
            throw new Error("归档批次不合法");
        return this.change((old) => {
            const previous = old.batches.find((entry) => entry.id === id);
            if (
                previous &&
                JSON.stringify(previous.items.map(({ id }) => id)) !==
                    JSON.stringify(items.map(({ id }) => id))
            )
                throw new Error("批次 ID 已用于其他会话");
            if (previous) return { next: old, result: previous };
            const batch: ArchiveBatch = {
                id,
                createdAt: Date.now() / 1000,
                finishedAt: null,
                items: items.map((item) => ({ ...item, status: "awaiting" })),
                ...(single ? { single } : {}),
            };
            return { next: { ...old, batches: [batch, ...old.batches] }, result: batch };
        });
    }

    private patch(
        id: string,
        transform: (batch: ArchiveBatch) => ArchiveBatch,
    ): Promise<ArchiveBatch> {
        return this.change((old) => {
            const batch = transform(this.requireBatch(id));
            return {
                next: { ...old, batches: old.batches.map((row) => (row.id === id ? batch : row)) },
                result: batch,
            };
        });
    }

    finish(id: string): Promise<ArchiveBatch> {
        return this.patch(id, (batch) => ({
            ...batch,
            finishedAt: batch.finishedAt ?? Date.now() / 1000,
        }));
    }

    private async set(
        id: string,
        item: ArchiveItem,
        status: ArchiveState,
        message?: string,
    ): Promise<ArchiveItem> {
        const next = { ...item, status, ...(message ? { message } : {}) };
        if (!message) delete next.message;
        await this.patch(id, (batch) => ({
            ...batch,
            items: batch.items.map((row) => (row.id === item.id ? next : row)),
        }));
        return next;
    }

    pending(id: string): boolean {
        return this.batches().some((batch) =>
            batch.items.some((item) => item.id === id && item.status === "pending"),
        );
    }

    async check(id: string): Promise<ArchiveBatch> {
        const scope = this.ports.scope();
        for (const item of this.requireBatch(id).items) {
            if (item.status !== "pending" || this.submitted.has(item.id)) continue;
            const archived = await this.ports.isArchived(item.id);
            this.requireScope(scope);
            if (archived) await this.set(id, item, "verified");
        }
        return this.requireBatch(id);
    }

    async single(candidate: ArchiveTarget): Promise<ArchiveItem> {
        const scope = this.ports.scope();
        const existing = this.batches().find(({ single, items }) => {
            const item = items[0];
            return (
                single &&
                item?.id === candidate.id &&
                item.version === candidate.version &&
                item.inputVersion === candidate.inputVersion &&
                item.sectionVersion === candidate.sectionVersion
            );
        });
        const batch = existing ?? (await this.start(crypto.randomUUID(), [candidate], true));
        this.requireScope(scope);
        const item = await this.submit(batch.id, candidate);
        this.requireScope(scope);
        await this.finish(batch.id);
        return item;
    }

    async submit(batchId: string, candidate: ArchiveTarget): Promise<ArchiveItem> {
        const batch = this.requireBatch(batchId);
        const item = batch.items.find((row) => row.id === candidate.id);
        if (
            !item ||
            item.version !== candidate.version ||
            item.inputVersion !== candidate.inputVersion ||
            item.sectionVersion !== candidate.sectionVersion
        )
            throw new Error("归档请求版本与批次登记不符");
        if (item.status !== "awaiting") return item;
        if (batch.finishedAt !== null || this.submitted.has(item.id) || this.pending(item.id)) {
            throw new Error("这条会话已有归档记录，请先核对");
        }
        this.submitted.add(item.id);
        try {
            return await this.submitItem(batchId, item);
        } finally {
            this.submitted.delete(item.id);
        }
    }

    private async submitItem(batch: string, item: ArchiveItem): Promise<ArchiveItem> {
        const scope = this.ports.scope();
        try {
            await this.ports.verify(item);
        } catch (error) {
            this.requireScope(scope);
            return this.set(
                batch,
                item,
                error instanceof NewInputError ? "newInput" : "rejected",
                String(error),
            );
        }
        this.requireScope(scope);
        let pending = await this.set(batch, item, "pending");
        try {
            this.requireScope(scope);
            await this.ports.archive(item);
        } catch (error) {
            this.requireScope(scope);
            if (error instanceof ArchivePreconditionError)
                return this.set(
                    batch,
                    item,
                    error instanceof NewInputError ? "newInput" : "rejected",
                    String(error),
                );
            pending = await this.set(batch, pending, "pending", String(error));
        }
        try {
            this.requireScope(scope);
            const archived = await this.ports.isArchived(item.id);
            this.requireScope(scope);
            if (archived) return await this.set(batch, pending, "archived");
        } catch (error) {
            this.requireScope(scope);
            return this.set(batch, pending, "pending", String(error));
        }
        return pending;
    }
}

import type { DraftRecord } from "./drafts.js";
