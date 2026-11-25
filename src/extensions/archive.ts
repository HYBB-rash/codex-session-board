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
export type ArchiveItem = ArchiveTarget & {
    status: ArchiveState;
    message?: string;
    checkedAt?: number;
};

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
    nativeVersion(version: string | null): string;
    verify(item: ArchiveItem): Promise<void>;
    isArchived(id: string): Promise<boolean>;
    archive(item: ArchiveItem): Promise<void>;
}

export class ArchivePreconditionError extends Error {}
export class NewInputError extends ArchivePreconditionError {}

export class ArchiveReadUnavailable extends Error {}

export function emptyLedger(): BoardLedger {
    return { batches: [], columnSorts: {} };
}

function archiveItemStatus(
    current: ArchiveItem,
    from: "awaiting" | "pending",
    status: ArchiveState,
    checkedAt: number,
    message?: string,
) {
    const next = { ...current, status, ...(message ? { message } : {}) };
    if (from === "pending" || status !== "pending") next.checkedAt = checkedAt;
    if (!message) delete next.message;
    return next;
}

function rejectedStatus(error: unknown): "newInput" | "rejected" {
    return error instanceof NewInputError ? "newInput" : "rejected";
}

export function createArchive(ports: ArchivePorts) {
    const submitted = new Set<string>();
    let tail: Promise<unknown> = Promise.resolve();

    function batches(): ArchiveBatch[] {
        return ports.ledger().batches;
    }

    function enqueue<T>(work: () => Promise<T>): Promise<T> {
        const operation = tail.then(work);
        tail = operation.then(
            () => undefined,
            () => undefined,
        );
        return operation;
    }

    async function change<T>(
        transform: (old: BoardLedger) => { next: BoardLedger; result: T },
    ): Promise<T> {
        const scope = ports.scope();
        return enqueue(async () => {
            requireScope(scope);
            const { next, result } = transform(ports.ledger());
            await ports.write(next);
            requireScope(scope);
            return result;
        });
    }

    function requireScope(scope: string): void {
        if (scope !== ports.scope()) throw new Error("归档期间 Codex 账号已变更");
    }

    function requireBatch(id: string): ArchiveBatch {
        const batch = batches().find((entry) => entry.id === id);
        if (!batch) throw new Error("找不到归档批次");
        return batch;
    }

    function start(id: string, items: ArchiveTarget[], single?: true): Promise<ArchiveBatch> {
        if (
            !/^[\da-f-]{36}$/i.test(id) ||
            items.length === 0 ||
            items.length !== new Set(items.map((item) => item.id)).size
        )
            throw new Error("归档批次不合法");
        return change((old) => {
            const previous = old.batches.find((entry) => entry.id === id);
            const targets = items.map(({ sectionVersion, ...item }, index) => ({
                ...item,
                sectionVersion:
                    previous?.items[index]?.sectionVersion === sectionVersion
                        ? sectionVersion
                        : ports.nativeVersion(sectionVersion),
            }));
            if (previous) {
                if (
                    previous.items.length !== targets.length ||
                    previous.items.some((item, index) => !sameArchiveTarget(item, targets[index]))
                )
                    throw new Error("归档批次名单已固定");
                return { next: old, result: previous };
            }
            const batch: ArchiveBatch = {
                id,
                createdAt: Date.now() / 1000,
                finishedAt: null,
                items: targets.map((item) => ({ ...item, status: "awaiting" })),
                ...(single ? { single } : {}),
            };
            return { next: { ...old, batches: [batch, ...old.batches] }, result: batch };
        });
    }

    function patch(
        id: string,
        transform: (batch: ArchiveBatch) => ArchiveBatch,
    ): Promise<ArchiveBatch> {
        return change((old) => {
            const batch = transform(requireBatch(id));
            return {
                next: { ...old, batches: old.batches.map((row) => (row.id === id ? batch : row)) },
                result: batch,
            };
        });
    }

    function finish(id: string): Promise<ArchiveBatch> {
        return patch(id, (batch) => ({
            ...batch,
            finishedAt: batch.finishedAt ?? Date.now() / 1000,
        }));
    }

    function set(
        id: string,
        item: ArchiveItem,
        from: "awaiting" | "pending",
        status: ArchiveState,
        message?: string,
    ): Promise<{ item: ArchiveItem; changed: boolean }> {
        return change<{ item: ArchiveItem; changed: boolean }>((old) => {
            const batch = requireBatch(id);
            const current = batch.items.find((row) => row.id === item.id);
            if (!current) throw new Error("会话不在此归档批次");
            if (current.status !== from || (from === "awaiting" && batch.finishedAt !== null))
                return { next: old, result: { item: current, changed: false } };
            const next = archiveItemStatus(current, from, status, Date.now() / 1000, message);
            const updated = {
                ...batch,
                items: batch.items.map((row) => (row.id === item.id ? next : row)),
            };
            return {
                next: {
                    ...old,
                    batches: old.batches.map((row) => (row.id === id ? updated : row)),
                },
                result: { item: next, changed: true },
            };
        });
    }

    function pending(id: string): boolean {
        return batches().some((batch) =>
            batch.items.some((item) => item.id === id && item.status === "pending"),
        );
    }

    async function checkPendingItem(batch: string, item: ArchiveItem, scope: string) {
        let archived: boolean;
        try {
            archived = await ports.isArchived(item.id);
        } catch (error) {
            requireScope(scope);
            if (error instanceof ArchiveReadUnavailable) return;
            throw error;
        }
        requireScope(scope);
        if (archived) await set(batch, item, "pending", "verified");
    }

    async function check(id: string): Promise<ArchiveBatch> {
        const scope = ports.scope();
        for (const item of requireBatch(id).items) {
            if (item.status !== "pending" || submitted.has(item.id)) continue;
            await checkPendingItem(id, item, scope);
        }
        requireScope(scope);
        return requireBatch(id);
    }

    async function single(candidate: ArchiveTarget): Promise<ArchiveItem> {
        const scope = ports.scope();
        const version = ports.nativeVersion(candidate.sectionVersion);
        const existing = batches().find(({ single, items }) => {
            const item = items[0];
            return (
                single &&
                item?.id === candidate.id &&
                item.version === candidate.version &&
                item.inputVersion === candidate.inputVersion &&
                item.sectionVersion === version
            );
        });
        const batch = existing ?? (await start(crypto.randomUUID(), [candidate], true));
        requireScope(scope);
        const item = await submit(batch.id, candidate);
        requireScope(scope);
        await finish(batch.id);
        return item;
    }

    async function submit(batchId: string, candidate: ArchiveTarget): Promise<ArchiveItem> {
        const batch = requireBatch(batchId);
        const item = batch.items.find((row) => row.id === candidate.id);
        if (
            !item ||
            item.version !== candidate.version ||
            item.inputVersion !== candidate.inputVersion ||
            (item.sectionVersion !== candidate.sectionVersion &&
                item.sectionVersion !== ports.nativeVersion(candidate.sectionVersion))
        )
            throw new Error("归档请求版本与批次登记不符");
        if (item.status !== "awaiting") return item;
        if (batch.finishedAt !== null || submitted.has(item.id) || pending(item.id)) {
            throw new Error("这条会话已有归档记录，请先核对");
        }
        submitted.add(item.id);
        try {
            return await submitItem(batchId, item);
        } finally {
            submitted.delete(item.id);
        }
    }

    async function submitItem(batch: string, item: ArchiveItem): Promise<ArchiveItem> {
        const scope = ports.scope();
        try {
            await ports.verify(item);
        } catch (error) {
            requireScope(scope);
            const rejected = await set(
                batch,
                item,
                "awaiting",
                rejectedStatus(error),
                String(error),
            );
            return rejected.item;
        }
        requireScope(scope);
        const claimed = await set(batch, item, "awaiting", "pending");
        if (!claimed.changed) return claimed.item;
        let pending = claimed.item;
        try {
            requireScope(scope);
            await ports.archive(item);
        } catch (error) {
            requireScope(scope);
            if (error instanceof ArchivePreconditionError) {
                const rejected = await set(
                    batch,
                    item,
                    "pending",
                    rejectedStatus(error),
                    String(error),
                );
                return rejected.item;
            }
            pending = (await set(batch, pending, "pending", "pending", String(error))).item;
        }
        return confirmSubmission(batch, pending, scope);
    }

    async function confirmSubmission(batch: string, item: ArchiveItem, scope: string) {
        try {
            requireScope(scope);
            const archived = await ports.isArchived(item.id);
            requireScope(scope);
            if (archived) return (await set(batch, item, "pending", "archived")).item;
        } catch (error) {
            requireScope(scope);
            return (await set(batch, item, "pending", "pending", String(error))).item;
        }
        return item;
    }

    return { batches, change, start, finish, pending, check, single, submit };
}

function sameArchiveTarget(item: ArchiveTarget, other: ArchiveTarget | undefined): boolean {
    return (
        item.id === other?.id &&
        item.title === other.title &&
        item.version === other.version &&
        item.inputVersion === other.inputVersion &&
        item.sectionVersion === other.sectionVersion
    );
}

import type { DraftRecord } from "./drafts.js";
