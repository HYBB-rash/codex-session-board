import type {
    DesktopConnection,
    HostConnection,
    HostId,
    SidebarContext,
} from "./desktop/connection.js";
import { labels, migrateArchiveRecord, migratedState } from "./placement.js";
import type { CardRecord } from "./records.js";

const uuid = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const revisionName = /^([0-9a-f-]{36})\.(\d+)\.json$/;
const claimName = /^([0-9a-f-]{36})\.(\d+)\.archive$/;
const settledClaimSuffix = ".settled";
const ensurePrivateDirectory =
    'umask 077; if [ ! -e "$1" ]; then /bin/mkdir -m 700 "$1" || [ -d "$1" ] || exit; fi; ' +
    '[ ! -L "$1" ] && [ -d "$1" ] && [ -O "$1" ] && /bin/chmod 700 "$1"';

const conflictName = "TaskConflict";

function taskConflict() {
    return Object.assign(new Error("任务已在另一端更新，请刷新后再保存"), {
        name: conflictName,
    });
}

export function isTaskConflict(error: unknown) {
    return error instanceof Error && error.name === conflictName;
}

interface Target {
    connection: HostConnection;
    directory: string;
    id: string;
    watchId: string;
}

interface Revision {
    record: CardRecord;
    operation: string;
    confirmedAt: number;
}

export interface ArchiveClaim {
    threadId: string;
    sequence: number;
    name: string;
    settled: boolean;
}

function revisionEntry(fileName: string) {
    const match = revisionName.exec(fileName);
    const threadId = match?.[1];
    if (!threadId || !uuid.test(threadId)) return;
    const version = Number(match?.[2]);
    if (Number.isSafeInteger(version) && version >= 1) return { threadId, version };
    return undefined;
}

function parseRevision(text: string, version: number): Revision {
    const value = JSON.parse(text) as Partial<Revision> | null;
    const record = value?.record;
    const state = migratedState(record?.state);
    if (
        !record ||
        record.version !== version ||
        typeof record.note !== "string" ||
        !Number.isSafeInteger(record.inputVersion) ||
        record.inputVersion < 0 ||
        (state !== null && (typeof state !== "string" || !Object.hasOwn(labels, state))) ||
        (record.deferred && typeof record.deferred.reason !== "string") ||
        typeof value?.operation !== "string" ||
        !uuid.test(value.operation) ||
        typeof value.confirmedAt !== "number" ||
        !Number.isFinite(value.confirmedAt)
    )
        throw new Error("目标主机的任务记录无效");
    return {
        record: migrateArchiveRecord(record),
        operation: value.operation,
        confirmedAt: value.confirmedAt,
    };
}

export function createTaskStore(desktop: DesktopConnection) {
    const targets = new Map<HostId, { epoch: number; scope: string; ready: Promise<Target> }>();
    const files = desktop.taskFiles;

    async function publish(
        connection: HostConnection,
        directory: string,
        name: string,
        data: string,
    ) {
        const destination = `${directory}/${name}`;
        const temporary = `${directory}/.draft-${crypto.randomUUID()}`;
        try {
            await files.write(connection, temporary, data);
            try {
                const result = await desktop.taskCommand(connection, directory, [
                    "/bin/ln",
                    temporary,
                    destination,
                ]);
                if (result.exitCode !== 0) {
                    return await files.read(connection, destination).catch(() => {
                        throw new Error(result.stderr?.trim() || "目标主机未提交任务记录");
                    });
                }
            } catch (error) {
                return await files.read(connection, destination).catch(() => {
                    throw error;
                });
            }
            return await files.read(connection, destination);
        } finally {
            await files.remove(connection, temporary).catch(() => undefined);
        }
    }

    async function open(
        connection: HostConnection,
        scope: string,
        identity: Pick<SidebarContext, "accountId" | "userId">,
    ): Promise<Target> {
        if (!/^[0-9a-f]{64}$/.test(scope)) throw new Error("任务账号标识无效");
        const home = await desktop.dataHome(connection, identity);
        const directory = `${home}/enhancer-tasks-v1-${scope}`;
        const result = await desktop.taskCommand(connection, home, [
            "/bin/sh",
            "-c",
            ensurePrivateDirectory,
            "enhancer-task-directory",
            directory,
        ]);
        if (result.exitCode !== 0) throw new Error("目标主机无法准备私有任务目录");
        const entries = await files.list(connection, directory);
        const id = entries.some(({ fileName, isFile }) => isFile && fileName === "identity")
            ? await files.read(connection, `${directory}/identity`)
            : await publish(connection, directory, "identity", crypto.randomUUID());
        if (!uuid.test(id)) throw new Error("目标主机的任务存储标识无效");
        const watchId = crypto.randomUUID();
        void files.watch(connection, directory, watchId).catch(() => undefined);
        return { connection, directory, id, watchId };
    }

    function target(connection: HostConnection, scope: string, identity: SidebarContext) {
        const previous = targets.get(connection.hostId);
        if (previous?.epoch === connection.epoch && previous.scope === scope) return previous.ready;
        if (previous?.epoch === connection.epoch) {
            void previous.ready
                .then(({ watchId }) => files.unwatch(connection, watchId))
                .catch(() => undefined);
        }
        const ready = open(connection, scope, identity).catch((error: unknown) => {
            if (targets.get(connection.hostId)?.ready === ready) targets.delete(connection.hostId);
            throw error;
        });
        targets.set(connection.hostId, { epoch: connection.epoch, scope, ready });
        return ready;
    }

    async function heads({ connection, directory }: Target) {
        const versions = new Map<string, number>();
        const claims = new Map<string, ArchiveClaim>();
        const names = new Set(
            (await files.list(connection, directory))
                .filter(({ isFile }) => isFile)
                .map(({ fileName }) => fileName),
        );
        for (const fileName of names) {
            const match = claimName.exec(fileName);
            const sequence = Number(match?.[2]);
            const threadId = match?.[1];
            if (
                threadId &&
                Number.isSafeInteger(sequence) &&
                sequence > (claims.get(threadId)?.sequence ?? -1)
            ) {
                claims.set(threadId, {
                    threadId,
                    sequence,
                    name: fileName,
                    settled: names.has(`${fileName}${settledClaimSuffix}`),
                });
            }
            const entry = revisionEntry(fileName);
            if (entry) {
                const { threadId, version } = entry;
                versions.set(threadId, Math.max(versions.get(threadId) ?? 0, version));
            }
        }
        return { versions, claims };
    }

    async function read({ connection, directory }: Target, thread: string, version: number) {
        if (!uuid.test(thread) || !Number.isSafeInteger(version) || version < 1)
            throw new Error("目标任务版本无效");
        return parseRevision(
            await files.read(connection, `${directory}/${thread}.${version}.json`),
            version,
        );
    }

    async function write(current: Target, thread: string, record: CardRecord) {
        if (!uuid.test(thread) || !Number.isSafeInteger(record.version) || record.version < 1)
            throw new Error("目标任务版本无效");
        const operation = crypto.randomUUID();
        const data = JSON.stringify({ record, operation, confirmedAt: Date.now() / 1000 });
        const committed = parseRevision(
            await publish(
                current.connection,
                current.directory,
                `${thread}.${record.version}.json`,
                data,
            ),
            record.version,
        );
        if (committed.operation !== operation) throw taskConflict();
        return committed;
    }

    async function archiveClaim(current: Target, threadId: string) {
        if (!uuid.test(threadId)) throw new Error("归档任务标识无效");
        const previous = (await heads(current)).claims.get(threadId);
        if (previous && !previous.settled)
            throw new Error("其他设备已提交归档意图，请核对实际结果");
        const operation = crypto.randomUUID();
        const sequence = (previous?.sequence ?? -1) + 1;
        const name = `${threadId}.${sequence}.archive`;
        if ((await publish(current.connection, current.directory, name, operation)) !== operation)
            throw new Error("其他设备已提交归档意图，请核对实际结果");
        return { threadId, sequence, name, settled: false };
    }

    async function settleArchiveClaim(current: Target, claim: ArchiveClaim) {
        if (claim.settled) return;
        await publish(
            current.connection,
            current.directory,
            `${claim.name}${settledClaimSuffix}`,
            "settled",
        );
    }

    return { target, heads, read, write, archiveClaim, settleArchiveClaim };
}
