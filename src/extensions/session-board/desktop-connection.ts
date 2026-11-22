/// <reference lib="dom" />

import type { DesktopCommand } from "./desktop-world.js";

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

export function interactiveThread(thread: Thread): boolean {
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
    if (thread.ephemeral || typeof thread.id !== "string" || !uuid.test(thread.id)) return false;
    return (
        thread.source === undefined ||
        (typeof thread.source === "string" && interactiveSources.includes(thread.source))
    );
}

export class DesktopConnection {
    private readonly hosts = new Map<
        HostId,
        { epoch: number; connected: boolean; catalog: boolean }
    >();
    private readonly pending = new Map<string, Pending>();

    constructor(private readonly transport: DesktopTransport) {}

    knownHosts(): HostId[] {
        const bootstrap = this.transport.bootstrap() as { catalogHostIds?: string[] } | null;
        if (!Array.isArray(bootstrap?.catalogHostIds)) return [...this.hosts.keys()];
        const catalog = new Set(
            bootstrap.catalogHostIds.map(hostId).filter((value) => value !== null),
        );
        for (const key of catalog) {
            if (!this.hosts.has(key)) {
                this.hosts.set(key, { epoch: 0, connected: true, catalog: true });
            }
        }
        for (const [key, state] of this.hosts) {
            if (state.catalog && !catalog.has(key)) this.setConnection(key, false, false);
            if (!state.catalog && catalog.has(key)) this.setConnection(key, true, true);
        }
        return [...this.hosts.keys()];
    }

    capture(key: HostId): HostConnection {
        const state = this.hosts.get(key);
        if (!state?.connected) throw new Error(`Codex 连接已断开 (${key})`);
        return { hostId: key, epoch: state.epoch };
    }

    assertCurrent(connection: HostConnection): void {
        if (this.capture(connection.hostId).epoch !== connection.epoch) {
            throw new Error("Codex 连接已变更，请刷新后重试");
        }
    }

    receive(value: unknown): NativePacket | undefined {
        if (typeof value !== "object" || value === null) return;
        const packet = value as NativePacket;
        const key = packet.hostId && hostId(packet.hostId);
        if (!key) return;
        if (packet.type === "codex-app-server-connection-changed") {
            this.setConnection(key, packet.state === "connected");
        }
        if (packet.type === "mcp-response" && packet.message?.id) {
            this.receiveReply(key, packet.message.id, packet.message);
        }
        return packet;
    }

    private setConnection(key: HostId, connected: boolean, catalog?: boolean): void {
        const previous = this.hosts.get(key);
        const present = catalog ?? previous?.catalog ?? false;
        if (previous?.connected === connected && previous.catalog === present) return;
        this.hosts.set(key, { epoch: (previous?.epoch ?? 0) + 1, connected, catalog: present });
        for (const [id, pending] of this.pending) {
            if (pending.connection.hostId === key) this.fail(id, "Codex 连接已变更");
        }
    }

    private receiveReply(
        key: HostId,
        id: string,
        reply: NonNullable<NativePacket["message"]>,
    ): void {
        const pending = this.pending.get(id);
        if (!pending) return;
        const error =
            pending.connection.hostId !== key
                ? "Codex 回执主机不一致"
                : reply.error
                  ? (reply.error.message ?? "Codex 请求失败")
                  : null;
        if (error) {
            this.fail(id, error);
            return;
        }
        window.clearTimeout(pending.timeout);
        this.pending.delete(id);
        pending.resolve(reply.result);
    }

    private fail(id: string, message: string): void {
        const pending = this.pending.get(id);
        if (!pending) return;
        window.clearTimeout(pending.timeout);
        this.pending.delete(id);
        pending.reject(new Error(message));
    }

    private request(
        connection: HostConnection,
        method: NativeMethod,
        params: object,
        timeoutMs = 15000,
    ): Promise<unknown> {
        this.assertCurrent(connection);
        const id = crypto.randomUUID();
        return new Promise((resolve, reject) => {
            const timeout = window.setTimeout(
                () => this.fail(id, `Codex ${method} 请求超时`),
                timeoutMs,
            );
            this.pending.set(id, { connection, timeout, resolve, reject });
            void this.transport
                .send({
                    type: "mcp-request",
                    hostId: connection.hostId,
                    request: { id, method, params },
                })
                .catch((error: unknown) => this.fail(id, String(error)));
        });
    }

    private async page<T>(
        connection: HostConnection,
        method: PageMethod,
        params: object,
        timeoutMs?: number,
    ): Promise<Page<T>> {
        const reply = (await this.request(connection, method, params, timeoutMs)) as Partial<
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

    async list(
        connection: HostConnection,
        archived: boolean,
        cursor: string | null,
        limit: number,
    ): Promise<Page<Thread>> {
        return this.page(connection, "thread/list", {
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

    projects(connection: HostConnection, cursor: string | null): Promise<Page<HostProject>> {
        return this.page(connection, "project/list", { cursor, limit: 100 });
    }

    turns(
        connection: HostConnection,
        threadId: ThreadId,
        itemsView: "summary" | "notLoaded",
        cursor: string | null = null,
        timeoutMs?: number,
    ): Promise<Page<Turn>> {
        return this.page(
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

    async rename(
        connection: HostConnection,
        threadId: ThreadId,
        name: string,
        timeoutMs?: number,
    ): Promise<void> {
        await this.request(connection, "thread/name/set", { threadId, name }, timeoutMs);
    }

    async assignProject(
        connection: HostConnection,
        threadId: ThreadId,
        projectId: string,
    ): Promise<void> {
        await this.request(connection, "thread/metadata/update", { threadId, projectId });
    }

    async setArchived(
        connection: HostConnection,
        threadId: ThreadId,
        archived: boolean,
    ): Promise<void> {
        await this.request(connection, archived ? "thread/archive" : "thread/unarchive", {
            threadId,
        });
    }

    async read(
        connection: HostConnection,
        threadId: ThreadId,
        includeTurns: boolean,
        timeoutMs?: number,
    ): Promise<Thread> {
        const reply = (await this.request(
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

    async context(): Promise<SidebarContext> {
        return (await this.transport.sidebar({ kind: "context" })) as SidebarContext;
    }

    async navigate(
        command: Extract<DesktopCommand, { kind: "open" | "new" | "settings" }>,
    ): Promise<unknown> {
        return this.transport.sidebar(command);
    }

    async move(
        connection: HostConnection,
        identity: Pick<SidebarContext, "accountId" | "userId">,
        threadId: ThreadId,
        from: LogicalSectionId | null,
        to: LogicalSectionId,
    ): Promise<void> {
        this.assertCurrent(connection);
        await this.transport.sidebar({
            kind: "move",
            ...identity,
            hostId: connection.hostId,
            threadId,
            from,
            to,
        });
        this.assertCurrent(connection);
    }
}
