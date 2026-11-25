import { readFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import {
    argumentsFor,
    assetsDirectory,
    type BoardRequest,
    loadOperations,
    type Operation,
    object,
} from "./operations.js";

export const resourceUri = "ui://session-board/index.html";
const mimeType = "text/html;profile=mcp-app";
const publisher = "board_view";
const description = "会话看板。通过当前 Codex 窗口的已连接主机读取和管理会话。";
const legacyHidden = new Set([
    "Archive",
    "ArchiveBatches",
    "ArchiveBatchStart",
    "ArchiveBatchFinish",
    "ArchiveBatchCheck",
    "NewReset",
    "Connect",
    "ProjectAssign",
    "SshSetup",
]);

interface McpOptions {
    request: BoardRequest;
    version: string;
    legacy?: boolean;
    assetsDir?: string;
}

function toolDescriptions(operations: Operation[], icon: string, legacy: boolean) {
    const icons = [{ src: `data:image/svg+xml;base64,${icon}`, mimeType: "image/svg+xml" }];
    const entries = operations.filter(({ action }) => !legacy || !legacyHidden.has(action));
    return [
        {
            name: publisher,
            title: "会话看板",
            description,
            icons,
            inputSchema: { type: "object", additionalProperties: false },
            annotations: { readOnlyHint: true },
            _meta: { ui: { resourceUri, visibility: ["app", "model"] } },
        },
        ...entries.map(({ action, tool, title, readOnly, schema }) => ({
            name: tool,
            title,
            description: title,
            inputSchema: schema,
            icons,
            annotations: {
                readOnlyHint: readOnly,
                destructiveHint: ["Archive", "PluginUpdateStart"].includes(action),
                openWorldHint: action === "PluginUpdateStart",
            },
            _meta: { ui: { visibility: ["app"] } },
        })),
    ];
}

function text(value: unknown) {
    return [{ type: "text", text: typeof value === "string" ? value : JSON.stringify(value) }];
}

function toolResult(value: unknown) {
    return { content: [{ type: "text", text: JSON.stringify(value) }], structuredContent: value };
}

function messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

async function callTool(
    operations: Operation[],
    options: McpOptions,
    parameters: Parameters<ReturnType<typeof operationsFor>>[1],
) {
    try {
        const name = parameters.name;
        if (name === publisher) return toolResult({ ready: true });
        const operation = operations.find(({ tool }) => name === tool);
        if (!operation || (options.legacy && legacyHidden.has(operation.action)))
            throw new Error("未知看板工具");
        const payload = argumentsFor(operation, parameters.arguments ?? {});
        const result = await options.request(operation.path, payload);
        return operation.action === "PreviewReply"
            ? { content: [], _meta: { previewReply: result } }
            : toolResult(result);
    } catch (error) {
        return { isError: true, content: text(messageOf(error)) };
    }
}

class RpcError extends Error {
    constructor(
        readonly code: number,
        message: string,
    ) {
        super(message);
    }
}

function readResource(uri: unknown, assets: string) {
    if (uri !== resourceUri) throw new RpcError(-32602, "未知界面资源");
    return {
        contents: [
            {
                uri: resourceUri,
                mimeType,
                text: readFileSync(join(assets, "mcp.html"), "utf8"),
                _meta: {
                    ui: {
                        prefersBorder: false,
                        csp: { connectDomains: [], resourceDomains: [], frameDomains: [] },
                    },
                    "openai/ui": {
                        preferredDisplayMode: "fullscreen",
                        availableDisplayModes: ["inline", "fullscreen"],
                    },
                },
            },
        ],
    };
}

function protocol(version: unknown) {
    const supported = ["2024-10-07", "2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
    return typeof version === "string" && supported.includes(version) ? version : "2025-11-25";
}

function operationsFor(options: McpOptions) {
    const assets = options.assetsDir ?? assetsDirectory;
    const operations = loadOperations(assets);
    const icon = readFileSync(join(assets, "board.svg")).toString("base64");
    const list = toolDescriptions(operations, icon, options.legacy ?? false);
    return (
        method: unknown,
        parameters: Partial<Record<"protocolVersion" | "uri" | "name" | "arguments", unknown>>,
    ): unknown => {
        switch (method) {
            case "initialize":
                return {
                    protocolVersion: protocol(parameters.protocolVersion),
                    capabilities: { tools: {}, resources: {} },
                    serverInfo: { name: "codex-session-board", version: options.version },
                    instructions: description,
                };
            case "ping":
                return {};
            case "tools/list":
                return { tools: list };
            case "tools/call":
                return callTool(operations, options, parameters);
            case "resources/list":
                return { resources: [{ uri: resourceUri, name: "会话看板", mimeType }] };
            case "resources/templates/list":
                return { resourceTemplates: [] };
            case "resources/read":
                return readResource(parameters.uri, assets);
            default:
                throw new RpcError(-32601, "未知 MCP 方法");
        }
    };
}

function rpcError(id: unknown, code: number, message: string) {
    return { jsonrpc: "2.0", id, error: { code, message } };
}

export function createMcpHandler(options: McpOptions) {
    const dispatch = operationsFor(options);
    return async (line: string) => {
        let value: unknown;
        try {
            value = JSON.parse(line);
        } catch (error) {
            return rpcError(null, -32700, messageOf(error));
        }
        const request = object<"id" | "jsonrpc" | "method" | "params">(value);
        const id = request?.id;
        if (
            request?.jsonrpc !== "2.0" ||
            typeof request.method !== "string" ||
            (id !== undefined && id !== null && typeof id !== "string" && typeof id !== "number")
        ) {
            return rpcError(null, -32600, "MCP 请求无效");
        }
        if (id === undefined) return undefined;
        try {
            const result = await dispatch(request.method, object(request.params) ?? {});
            return { jsonrpc: "2.0", id, result };
        } catch (error) {
            return rpcError(id, error instanceof RpcError ? error.code : -32603, messageOf(error));
        }
    };
}

export async function serveMcp(
    options: McpOptions,
    input: NodeJS.ReadableStream,
    output: NodeJS.WritableStream,
) {
    const lines = createInterface({ input, crlfDelay: Infinity });
    const answer = createMcpHandler(options);
    for await (const line of lines) {
        const response = await answer(line);
        if (response !== undefined) output.write(`${JSON.stringify(response)}\n`);
    }
}
