/// <reference lib="dom" />

import type { DesktopCommand } from "./world.js";

declare const hostBrand: unique symbol;
declare const threadBrand: unique symbol;
declare const logicalBrand: unique symbol;
declare const replicaBrand: unique symbol;

export type HostId = string & { readonly [hostBrand]: true };
export type ThreadId = string & { readonly [threadBrand]: true };
export type LogicalSectionId = string & { readonly [logicalBrand]: true };
export type HostSectionId = string & { readonly [replicaBrand]: true };
export const localHostId = "local" as HostId;
const interactiveSources = ["cli", "vscode", "appServer"];
const threadIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

export interface ThreadRef {
    hostId: HostId;
    threadId: ThreadId;
}

export interface Thread {
    id: ThreadId;
    name: string | null;
    preview: string;
    cwd: string;
    projectId: string | null;
    updatedAt: number;
    createdAt: number;
    recencyAt: number | null;
    section: { id: HostSectionId; name: string } | null;
    sectionEnteredAt: number | null;
    ephemeral: boolean;
    source?: string | { custom: string } | { subAgent: unknown };
    status: { type: string; activeFlags?: string[] };
    turns: Turn[];
}

export interface Turn {
    id: string;
    status: string;
    itemsView?: string;
    startedAt?: number | null;
    completedAt?: number | null;
    items: { id: string; type: string; phase?: string; text?: string }[];
}

export interface HostConnection {
    hostId: HostId;
    epoch: number;
}

export interface SidebarContext {
    accountId: string;
    userId: string;
    remoteHostNames?: Readonly<Record<string, string>>;
    sections: {
        id: LogicalSectionId;
        name: string;
        hostSectionIds?: Record<string, HostSectionId>;
        itemKeys: string[];
    }[];
}

interface NativePacket {
    type?: string;
    hostId?: string;
    state?: string;
    method?: string;
    params?: { threadId?: string };
    message?: { id?: string; result?: unknown; error?: { message?: string } };
}

interface Pending {
    connection: HostConnection;
    timeout: number;
    resolve(result: unknown): void;
    reject(error: Error): void;
}

interface Page<T> {
    data: T[];
    nextCursor: string | null;
}

type PageMethod = "thread/list" | "thread/turns/list" | "project/list";
type NativeMethod =
    | PageMethod
    | "thread/read"
    | "thread/name/set"
    | "thread/metadata/update"
    | "thread/archive"
    | "thread/unarchive";

export interface HostProject {
    id: string;
    name: string;
    roots: { path: string }[];
}

export interface DesktopTransport {
    bootstrap(): unknown;
    send(value: object): Promise<unknown>;
    sidebar(command: DesktopCommand): Promise<unknown>;
}

export function hostId(value: string): HostId | null {
    return value === "local" || /^remote-(ssh|wsl|control)[-:]/.test(value)
        ? (value as HostId)
        : null;
}

export function cardId(ref: ThreadRef): string {
    return ref.hostId === "local" ? ref.threadId : JSON.stringify([ref.hostId, ref.threadId]);
}

export function cardThread(id: string): ThreadRef {
    if (threadIdPattern.test(id)) return { hostId: localHostId, threadId: id as ThreadId };
    if (!id.startsWith("[")) throw new Error("看板会话 ID 无效");
    const pair: unknown = JSON.parse(id);
    if (Array.isArray(pair) && pair.length === 2) {
        const [host, thread] = pair;
        const target = typeof host === "string" ? hostId(host) : null;
        if (target && typeof thread === "string" && threadIdPattern.test(thread)) {
            const reference = { hostId: target, threadId: thread as ThreadId };
            if (cardId(reference) === id) return reference;
        }
    }
    throw new Error("看板会话 ID 无效");
}

export function interactiveThread(thread: Thread): boolean {
    if (thread.ephemeral || typeof thread.id !== "string" || !threadIdPattern.test(thread.id))
        return false;
    return (
        thread.source === undefined ||
        (typeof thread.source === "string" && interactiveSources.includes(thread.source))
    );
}

export type DesktopConnection = ReturnType<typeof createDesktopConnection>;

export function createDesktopConnection(transport: DesktopTransport) {
    const hosts = new Map<HostId, { epoch: number; connected: boolean; catalog: boolean }>();
    const requests = new Map<string, Pending>();

    function knownHosts(): HostId[] {
        const bootstrap = transport.bootstrap() as { catalogHostIds?: string[] } | null;
        if (!Array.isArray(bootstrap?.catalogHostIds)) return [...hosts.keys()];
        const catalog = new Set(
            bootstrap.catalogHostIds.map(hostId).filter((value) => value !== null),
        );
        synchronizeCatalog(catalog);
        return [...hosts.keys()];
    }

    function synchronizeCatalog(catalog: ReadonlySet<HostId>) {
        for (const key of catalog) {
            if (!hosts.has(key)) {
                hosts.set(key, { epoch: 0, connected: true, catalog: true });
            }
        }
        for (const [key, state] of hosts) {
            if (state.catalog && !catalog.has(key)) setConnection(key, false, false);
            if (!state.catalog && catalog.has(key)) setConnection(key, true, true);
        }
    }

    function capture(key: HostId): HostConnection {
        const state = hosts.get(key);
        if (!state?.connected) throw new Error(`Codex 连接已断开 (${key})`);
        return { hostId: key, epoch: state.epoch };
    }

    function assertCurrent(connection: HostConnection): void {
        if (capture(connection.hostId).epoch !== connection.epoch) {
            throw new Error("Codex 连接已变更，请刷新后重试");
        }
    }

    function receive(value: unknown): NativePacket | undefined {
        if (typeof value !== "object" || value === null) return;
        const packet = value as NativePacket;
        const key = typeof packet.hostId === "string" && hostId(packet.hostId);
        if (!key) return;
        if (packet.type === "codex-app-server-connection-changed") {
            setConnection(key, packet.state === "connected");
        }
        if (packet.type === "mcp-response" && typeof packet.message?.id === "string") {
            receiveReply(key, packet.message.id, packet.message);
        }
        return packet;
    }

    function setConnection(key: HostId, connected: boolean, catalog?: boolean): void {
        const previous = hosts.get(key);
        const present = catalog ?? previous?.catalog ?? false;
        if (previous?.connected === connected && previous.catalog === present) return;
        hosts.set(key, { epoch: (previous?.epoch ?? 0) + 1, connected, catalog: present });
        for (const [id, pending] of requests) {
            if (pending.connection.hostId === key) fail(id, "Codex 连接已变更");
        }
    }

    function receiveReply(
        key: HostId,
        id: string,
        reply: NonNullable<NativePacket["message"]>,
    ): void {
        const pending = requests.get(id);
        if (!pending) return;
        const error =
            pending.connection.hostId !== key
                ? "Codex 回执主机不一致"
                : reply.error
                  ? (reply.error.message ?? "Codex 请求失败")
                  : null;
        if (error) {
            fail(id, error);
            return;
        }
        window.clearTimeout(pending.timeout);
        requests.delete(id);
        pending.resolve(reply.result);
    }

    function fail(id: string, message: string): void {
        const pending = requests.get(id);
        if (!pending) return;
        window.clearTimeout(pending.timeout);
        requests.delete(id);
        pending.reject(new Error(message));
    }

    function request(
        connection: HostConnection,
        method: NativeMethod,
        params: object,
        timeoutMs = 15000,
    ): Promise<unknown> {
        assertCurrent(connection);
        const id = crypto.randomUUID();
        return new Promise((resolve, reject) => {
            const timeout = window.setTimeout(
                () => fail(id, `Codex ${method} 请求超时`),
                timeoutMs,
            );
            requests.set(id, { connection, timeout, resolve, reject });
            void transport
                .send({
                    type: "mcp-request",
                    hostId: connection.hostId,
                    request: { id, method, params },
                })
                .catch((error: unknown) => fail(id, String(error)));
        });
    }

    async function page<T>(
        connection: HostConnection,
        method: PageMethod,
        params: object,
        timeoutMs?: number,
    ): Promise<Page<T>> {
        const reply = (await request(connection, method, params, timeoutMs)) as Partial<
            Page<T>
        > | null;
        if (
            !Array.isArray(reply?.data) ||
            (reply.nextCursor != null && typeof reply.nextCursor !== "string")
        ) {
            throw new Error(`Codex ${method} 分页回执无效`);
        }
        return { data: reply.data, nextCursor: reply.nextCursor ?? null };
    }

    async function list(
        connection: HostConnection,
        archived: boolean,
        cursor: string | null,
        limit: number,
    ): Promise<Page<Thread>> {
        return page(connection, "thread/list", {
            cursor,
            limit,
            archived,
            sortKey: "updated_at",
            sortDirection: "desc",
            sourceKinds: interactiveSources,
            modelProviders: [],
            useStateDbOnly: true,
        });
    }

    function projects(
        connection: HostConnection,
        cursor: string | null,
    ): Promise<Page<HostProject>> {
        return page(connection, "project/list", { cursor, limit: 100 });
    }

    function turns(
        connection: HostConnection,
        threadId: ThreadId,
        itemsView: "summary" | "notLoaded",
        cursor: string | null = null,
        timeoutMs?: number,
    ): Promise<Page<Turn>> {
        return page(
            connection,
            "thread/turns/list",
            {
                threadId,
                limit: 1,
                sortDirection: "desc",
                itemsView,
                cursor,
            },
            timeoutMs,
        );
    }

    async function rename(
        connection: HostConnection,
        threadId: ThreadId,
        name: string,
        timeoutMs?: number,
    ): Promise<void> {
        await request(connection, "thread/name/set", { threadId, name }, timeoutMs);
    }

    async function assignProject(
        connection: HostConnection,
        threadId: ThreadId,
        projectId: string,
    ): Promise<void> {
        await request(connection, "thread/metadata/update", { threadId, projectId });
    }

    async function setArchived(
        connection: HostConnection,
        threadId: ThreadId,
        archived: boolean,
    ): Promise<void> {
        await request(connection, archived ? "thread/archive" : "thread/unarchive", {
            threadId,
        });
    }

    async function read(
        connection: HostConnection,
        threadId: ThreadId,
        includeTurns: boolean,
        timeoutMs?: number,
    ): Promise<Thread> {
        const reply = (await request(
            connection,
            "thread/read",
            { threadId, includeTurns },
            timeoutMs,
        )) as {
            thread?: Thread;
        } | null;
        if (reply?.thread?.id !== threadId) throw new Error("Codex 回执会话不一致");
        return reply.thread;
    }

    async function context(): Promise<SidebarContext> {
        return (await transport.sidebar({ kind: "context" })) as SidebarContext;
    }

    async function navigate(
        command: Extract<DesktopCommand, { kind: "open" | "new" | "settings" | "draft" }>,
    ): Promise<unknown> {
        return transport.sidebar(command);
    }

    async function move(
        connection: HostConnection,
        identity: Pick<SidebarContext, "accountId" | "userId">,
        threadId: ThreadId,
        from: LogicalSectionId | null,
        to: LogicalSectionId,
    ): Promise<void> {
        assertCurrent(connection);
        await transport.sidebar({
            kind: "move",
            ...identity,
            hostId: connection.hostId,
            threadId,
            from,
            to,
        });
        assertCurrent(connection);
    }

    return {
        knownHosts,
        capture,
        assertCurrent,
        receive,
        list,
        projects,
        turns,
        rename,
        assignProject,
        setArchived,
        read,
        context,
        navigate,
        move,
    };
}
