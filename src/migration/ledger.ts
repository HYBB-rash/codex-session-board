import type {
    ArchiveItem,
    ArchiveState,
    BoardLedger,
} from "../extensions/session-board/archive.js";
import { projectPendingDrafts } from "./drafts.js";
import type {
    LegacyAccountBinding,
    LegacyImportDatabase,
    MigratedBoard,
    OldRow,
} from "./projection.js";

const statuses: ArchiveState[] = [
    "awaiting",
    "pending",
    "archived",
    "verified",
    "newInput",
    "rejected",
];
const threadIdPattern = /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/;

function identify(raw: string, database: LegacyImportDatabase, binding: LegacyAccountBinding) {
    if (threadIdPattern.test(raw)) return { host: database.hostId, thread: raw };
    if (!raw.startsWith("remote:")) return undefined;
    const key: unknown = JSON.parse(raw.slice("remote:".length));
    if (!Array.isArray(key) || key.length !== 3 || key[0] !== "codex") return undefined;
    const [, source, thread] = key;
    if (
        typeof source !== "string" ||
        typeof thread !== "string" ||
        !threadIdPattern.test(thread) ||
        !Object.hasOwn(binding.hosts, source)
    ) {
        return undefined;
    }
    const host = binding.hosts[source];
    return host ? { host, thread } : undefined;
}

function itemFrom(
    row: OldRow,
    database: LegacyImportDatabase,
    binding: LegacyAccountBinding,
): ArchiveItem | undefined {
    if (typeof row.id !== "string" || typeof row.target !== "string") return undefined;
    const id = identify(row.id, database, binding);
    const target = JSON.parse(row.target) as Partial<ArchiveItem> | null;
    const status = statuses.find((value) => value === row.status);
    if (
        !id ||
        !status ||
        target?.id !== row.id ||
        typeof target.title !== "string" ||
        typeof target.version !== "number" ||
        !Number.isSafeInteger(target.version) ||
        target.version < 0 ||
        typeof target.inputVersion !== "number" ||
        !Number.isSafeInteger(target.inputVersion) ||
        target.inputVersion < 0 ||
        (target.sectionVersion !== null && typeof target.sectionVersion !== "string")
    ) {
        return undefined;
    }
    return {
        id: id.host === "local" ? id.thread : JSON.stringify([id.host, id.thread]),
        title: target.title,
        version: target.version,
        inputVersion: target.inputVersion,
        sectionVersion: target.sectionVersion,
        status,
        ...(typeof row.reason === "string" ? { message: row.reason } : {}),
    };
}

export function projectLedger(board: MigratedBoard, binding: LegacyAccountBinding): BoardLedger {
    const batches = board.databases.flatMap((database) =>
        database.archiveBatches.flatMap((row) => {
            if (typeof row.id !== "string" || typeof row.created_at !== "number") return [];
            const items = database.archiveItems
                .filter((item) => item.batch_id === row.id)
                .sort((left, right) => Number(left.position) - Number(right.position))
                .flatMap((item) => itemFrom(item, database, binding) ?? []);
            if (items.length === 0) return [];
            return [
                {
                    id:
                        database.hostId === "local"
                            ? row.id
                            : JSON.stringify(["legacy", database.sourceHostId, row.id]),
                    createdAt: row.created_at,
                    finishedAt: typeof row.finished_at === "number" ? row.finished_at : null,
                    items,
                },
            ];
        }),
    );
    return {
        batches: batches.sort((left, right) => right.createdAt - left.createdAt),
        columnSorts: board.columnSorts,
        pendingDrafts: board.databases.flatMap((database) =>
            projectPendingDrafts(database, binding),
        ),
    };
}
