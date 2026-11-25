import { randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { mkdir, readFile, realpath } from "node:fs/promises";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { join } from "node:path";
import { readCache, writeCache } from "./cache.js";
import {
    argumentsFor,
    assetsDirectory,
    type BoardRequest,
    loadOperations,
    type Operation,
} from "./operations.js";
import { type ServiceIdentity, serviceBuild, serviceConfiguration } from "./service/identity.js";
import { processStartIdentity } from "./service/process.js";
import {
    type Receipt,
    releaseReceipt,
    requireInactiveReceipt,
    writeReceipt,
} from "./service/receipt.js";

export class HostUnavailableError extends Error {
    constructor() {
        super("请打开装有 Enhancer 的 Codex 窗口");
    }
}

export interface BoardServerOptions {
    request: BoardRequest;
    stateDir: string;
    port?: number;
    assetsDir?: string;
    cacheAccount?: () => string | undefined;
    cacheError?: (error: unknown) => void;
}

export interface BoardServer {
    url: string;
    close(): Promise<void>;
}

const assets: Readonly<Record<string, readonly [string, string]>> = {
    "/": ["index.html", "text/html; charset=utf-8"],
    "/board.js": ["board.js", "application/javascript; charset=utf-8"],
    "/manifest.webmanifest": ["manifest.webmanifest", "application/manifest+json; charset=utf-8"],
    "/assets/board.svg": ["board.svg", "image/svg+xml"],
    "/sw.js": ["sw.js", "application/javascript; charset=utf-8"],
};

class RequestError extends Error {
    constructor(
        readonly status: number,
        message: string,
    ) {
        super(message);
    }
}

interface Runtime {
    url: URL;
    nonce: string;
    receipt: Receipt;
    assetsDir: string;
    options: BoardServerOptions;
    operations: Operation[];
    stop(): Promise<void>;
    cachedAt: number;
}

function headers(response: ServerResponse, nonce: string) {
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("X-Content-Type-Options", "nosniff");
    response.setHeader("X-Frame-Options", "DENY");
    response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
    response.setHeader("Referrer-Policy", "no-referrer");
    response.setHeader(
        "Content-Security-Policy",
        `default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; manifest-src 'self'; worker-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'; navigate-to 'self'`,
    );
    response.setHeader("X-Board-Instance", nonce);
}

function send(response: ServerResponse, status: number, data: string | Buffer, mime: string) {
    response.writeHead(status, { "Content-Type": mime }).end(data);
}

function json(response: ServerResponse, status: number, data: unknown) {
    send(response, status, JSON.stringify(data), "application/json; charset=utf-8");
}

function tokenMatches(received: string | string[] | undefined, expected: string): boolean {
    if (typeof received !== "string") return false;
    const provided = Buffer.from(received);
    const token = Buffer.from(expected);
    return provided.length === token.length && timingSafeEqual(provided, token);
}

function enforceOrigin(request: IncomingMessage, runtime: Runtime): boolean {
    const { headers: incoming, socket } = request;
    if (
        socket.remoteAddress !== "127.0.0.1" ||
        incoming.host !== runtime.url.host ||
        (incoming.origin !== undefined && incoming.origin !== runtime.url.origin) ||
        incoming["sec-fetch-site"] === "cross-site"
    ) {
        throw new RequestError(403, "请从本地看板入口访问");
    }
    return tokenMatches(incoming.authorization, `Bearer ${runtime.receipt.clientToken}`);
}

async function body(request: IncomingMessage, max: number): Promise<unknown> {
    const size = Number(request.headers["content-length"]);
    if (
        !/^application\/json(?:;.*)?$/i.test(request.headers["content-type"] ?? "") ||
        !Number.isSafeInteger(size) ||
        size < 1 ||
        size > max
    ) {
        throw new RequestError(400, "请求格式或长度无效");
    }
    const chunks: Buffer[] = [];
    let length = 0;
    for await (const chunk of request as AsyncIterable<Buffer>) {
        length += chunk.length;
        if (length > max) throw new RequestError(413, "请求太长");
        chunks.push(chunk);
    }
    try {
        return JSON.parse(Buffer.concat(chunks).toString("utf8")) as unknown;
    } catch {
        throw new RequestError(400, "JSON 请求无效");
    }
}

function persist(runtime: Runtime, account: string | undefined, value: unknown) {
    if (!account || Date.now() - runtime.cachedAt < 4000) return;
    if (account !== runtime.options.cacheAccount?.()) return;
    try {
        writeCache(runtime.options.stateDir, account, value);
        runtime.cachedAt = Date.now();
    } catch (error) {
        runtime.options.cacheError?.(error);
    }
}

async function dispatch(
    runtime: Runtime,
    operation: Operation,
    payload: unknown,
): Promise<unknown> {
    const argument = argumentsFor(operation, payload);
    const account = runtime.options.cacheAccount?.();
    let result: unknown;
    try {
        result = await runtime.options.request(operation.path, argument);
    } catch (error) {
        if (!(error instanceof HostUnavailableError)) throw error;
        if (account !== runtime.options.cacheAccount?.())
            throw new RequestError(409, "Codex 账号已变更，请重新读取看板");
        const cached =
            operation.action === "Snapshot" && readCache(runtime.options.stateDir, account);
        if (cached) return cached;
        throw new RequestError(503, error.message);
    }
    if (account !== runtime.options.cacheAccount?.())
        throw new RequestError(409, "Codex 账号已变更，请重新读取看板");
    if (operation.action === "Snapshot") persist(runtime, account, result);
    return result;
}

async function respondAsset(
    runtime: Runtime,
    path: string,
    response: ServerResponse,
    client: boolean,
): Promise<boolean> {
    if (path === "/api/info") {
        json(response, 200, {
            app: runtime.receipt.app,
            instance: runtime.receipt.instance,
            url: runtime.url.href,
            requestToken: runtime.nonce,
            ...(client ? { pid: runtime.receipt.pid, service: runtime.receipt.service } : {}),
        });
        return true;
    }
    const asset = Object.hasOwn(assets, path) ? assets[path] : undefined;
    if (!asset) return false;
    send(response, 200, await readFile(join(runtime.assetsDir, asset[0])), asset[1]);
    return true;
}

async function respond(runtime: Runtime, request: IncomingMessage, response: ServerResponse) {
    headers(response, runtime.receipt.instance);
    const client = enforceOrigin(request, runtime);
    const path = request.url?.split("?")[0] ?? "";
    if (!path.startsWith("/") || path.startsWith("//")) throw new RequestError(404, "接口不存在");
    if (request.method === "GET" && (await respondAsset(runtime, path, response, client))) return;
    if (
        request.method === "POST" &&
        !client &&
        !tokenMatches(request.headers["x-csrf-token"], runtime.nonce)
    ) {
        throw new RequestError(403, "看板请求令牌已失效，请重新打开页面");
    }
    if (path === "/api/service/stop" && request.method === "POST" && client) {
        const match = (await body(request, 1024)) as Partial<Receipt>;
        if (
            match?.instance !== runtime.receipt.instance ||
            (match.pid !== undefined && match.pid !== runtime.receipt.pid)
        ) {
            throw new RequestError(409, "看板服务实例已变更");
        }
        response.once("finish", () => {
            void runtime.stop().catch(runtime.options.cacheError ?? console.error);
        });
        json(response, 202, { stopping: true, instance: runtime.receipt.instance });
        return;
    }
    const operation = runtime.operations.find(
        (item) => item.path === path && item.method === request.method,
    );
    if (!operation) throw new RequestError(404, "接口不存在");
    const payload =
        operation.method === "POST"
            ? await body(request, operation.action === "ArchiveBatchStart" ? 1024 * 1024 : 12000)
            : {};
    json(
        response,
        operation.action === "Refresh" ? 202 : 200,
        await dispatch(runtime, operation, payload),
    );
}

function listen(server: Server, port: number): Promise<URL> {
    return new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(port, "127.0.0.1", () => {
            server.removeListener("error", reject);
            const address = server.address();
            if (typeof address !== "object" || address === null)
                reject(new Error("看板未能取得本机监听地址"));
            else resolve(new URL(`http://127.0.0.1:${address.port}/`));
        });
    });
}

function closeServer(server: Server): Promise<void> {
    return new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
    );
}

async function prepareService(stateDir: string, portOverride?: number): Promise<ServiceIdentity> {
    await mkdir(stateDir, { recursive: true, mode: 0o700 });
    const directory = await realpath(stateDir);
    requireInactiveReceipt(directory);
    const config = serviceConfiguration(directory);
    return {
        directory,
        build: serviceBuild(),
        config: config.digest,
        port: portOverride ?? config.port,
    };
}

function createReceipt(url: URL, identity: ServiceIdentity): Receipt {
    const processStart = processStartIdentity(process.pid);
    return {
        app: "codex-session-board",
        url: url.href,
        clientToken: randomBytes(32).toString("hex"),
        instance: randomUUID(),
        pid: process.pid,
        ...(processStart === null ? {} : { processStart }),
        service: identity,
    };
}

export async function startBoardServer(options: BoardServerOptions): Promise<BoardServer> {
    const identity = await prepareService(options.stateDir, options.port);
    const { directory, port } = identity;
    const assetsDir = options.assetsDir ?? assetsDirectory;
    const operations = loadOperations(assetsDir);
    let runtime: Runtime | undefined;
    const server = createServer(
        { requestTimeout: 20000, maxHeaderSize: 8192 },
        (request, response) => {
            if (!runtime) return json(response, 503, { error: "看板服务正在准备" });
            void respond(runtime, request, response).catch((error: unknown) => {
                const status = error instanceof RequestError ? error.status : 400;
                const message = error instanceof Error ? error.message : String(error);
                json(response, status, { error: message });
            });
        },
    );
    const url = await listen(server, port);
    const receipt = createReceipt(url, identity);
    try {
        writeReceipt(directory, receipt);
    } catch (error) {
        await closeServer(server);
        throw error;
    }
    let stopped: Promise<void> | undefined;
    const close = () => {
        stopped ??= closeServer(server).then(() => releaseReceipt(directory, receipt));
        return stopped;
    };
    runtime = {
        url,
        receipt,
        nonce: randomBytes(32).toString("hex"),
        assetsDir,
        options: { ...options, stateDir: directory },
        operations,
        stop: close,
        cachedAt: 0,
    };
    server.unref();
    return { url: url.href, close };
}
