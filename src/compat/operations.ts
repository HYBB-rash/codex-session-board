import { readFileSync } from "node:fs";
import { join } from "node:path";

export type BoardRequest = (path: string, body?: object) => Promise<unknown>;
export const assetsDirectory = join(__dirname, "..", "board-assets");

type Value<Key extends string = never> = Record<string, unknown> & Partial<Record<Key, unknown>>;
type Method = "GET" | "POST";
type Schema = {
    type: string | string[];
    enum?: readonly unknown[];
    const?: unknown;
    maxLength?: number;
    minimum?: number;
    minItems?: number;
    items?: Schema;
    properties?: Record<string, Schema>;
    required?: string[];
    additionalProperties?: false;
};

export interface Operation {
    action: string;
    method: Method;
    path: string;
    tool: string;
    title: string;
    readOnly: boolean;
    schema: Schema;
}

const text = { type: "string" };
const integer = { type: "integer", minimum: 0 };
const nullableText = { type: ["string", "null"] };
const confirmed = { type: "boolean", const: true };
const states = ["unknown", "in_progress", "review", "blocked", "to_archive", "completed"];

function fields(properties: Record<string, Schema> = {}, required: string[] = []): Schema {
    return { type: "object", properties, required, additionalProperties: false };
}

const cardVersion = {
    id: text,
    version: integer,
    inputVersion: integer,
    sectionVersion: nullableText,
};
const cardRequired = ["id", "version", "inputVersion"];
const byId = fields({ id: text }, ["id"]);
const schemas: Record<string, Schema> = {
    Snapshot: fields(),
    Update: fields(
        {
            ...cardVersion,
            state: { ...nullableText, enum: [...states, null] },
            note: { ...text, maxLength: 2000 },
        },
        [...cardRequired, "state"],
    ),
    Sort: fields(
        {
            state: { ...text, enum: states },
            field: { ...text, enum: ["updated_at"] },
            direction: { ...text, enum: ["asc", "desc"] },
            version: integer,
        },
        ["state", "field", "direction", "version"],
    ),
    Archive: fields({ ...cardVersion, batchId: text }, cardRequired),
    ArchiveBatches: fields(),
    ArchiveBatchStart: fields(
        {
            id: text,
            items: {
                type: "array",
                minItems: 1,
                items: fields({ ...cardVersion, title: { ...text, maxLength: 500 } }, [
                    ...cardRequired,
                    "title",
                ]),
            },
        },
        ["id", "items"],
    ),
    ArchiveBatchFinish: byId,
    ArchiveBatchCheck: byId,
    Open: byId,
    PreviewReply: byId,
    New: fields({
        projectId: text,
        requestId: text,
        hostId: text,
        operationId: text,
        consent: text,
    }),
    NewReset: fields({ projectId: text, requestId: text, confirmed }, [
        "projectId",
        "requestId",
        "confirmed",
    ]),
    ProjectAssign: fields({ id: text, projectId: text, confirmed }, [
        "id",
        "projectId",
        "confirmed",
    ]),
    Connect: fields({ hostId: text }, ["hostId"]),
    SshSetup: fields(
        {
            action: {
                ...text,
                enum: ["status", "preview", "apply", "decline", "restore", "recheck"],
            },
            target: fields({ hostname: text, user: text, port: text, directory: text }, [
                "hostname",
                "user",
                "port",
                "directory",
            ]),
            previewId: text,
            targetId: text,
            alias: text,
        },
        ["action"],
    ),
    Refresh: fields(),
    PluginUpdateStatus: fields(),
    PluginUpdateStart: fields(),
};

export function object<Key extends string = never>(value: unknown): Value<Key> | null {
    return value !== null && typeof value === "object" && !Array.isArray(value)
        ? (value as Value<Key>)
        : null;
}

function readOperation(raw: unknown): Operation {
    const value = object(raw);
    if (!value) throw new Error("看板操作登记格式无效");
    const { action, method, path, tool, title, readOnly } = value;
    if (
        typeof action !== "string" ||
        (method !== "GET" && method !== "POST") ||
        typeof path !== "string" ||
        !path.startsWith("/api/") ||
        typeof tool !== "string" ||
        !tool.startsWith("board_") ||
        typeof title !== "string" ||
        typeof readOnly !== "boolean"
    ) {
        throw new Error("看板操作登记值无效");
    }
    const schema = Object.hasOwn(schemas, action) ? schemas[action] : undefined;
    if (!schema) throw new Error(`看板操作缺少输入定义 ${action}`);
    return { action, method, path, tool, title, readOnly, schema };
}

export function loadOperations(assets = assetsDirectory): Operation[] {
    const registry: unknown = JSON.parse(
        readFileSync(join(assets, "board-operations.json"), "utf8"),
    );
    if (!Array.isArray(registry)) throw new Error("看板操作登记格式无效");
    const operations = registry.map(readOperation);
    const identifiers = (key: "action" | "tool") =>
        new Set(operations.map((operation) => operation[key])).size;
    const routes = new Set(operations.map(({ method, path }) => `${method} ${path}`));
    if (
        [identifiers("action"), identifiers("tool"), routes.size, operations.length].some(
            (count) => count !== Object.keys(schemas).length,
        )
    ) {
        throw new Error("看板操作登记必须覆盖全部共享操作");
    }
    return operations;
}

function matchType(value: unknown, type: string): boolean {
    if (type === "integer") return Number.isSafeInteger(value);
    if (type === "array") return Array.isArray(value);
    if (type === "object") return object(value) !== null;
    if (type === "null") return value === null;
    return typeof value === type;
}

function matches(schema: Schema, value: unknown): boolean {
    if (![schema.type].flat().some((type) => matchType(value, type))) return false;
    if (schema.enum && !schema.enum.includes(value)) return false;
    if (schema.const !== undefined && schema.const !== value) return false;
    if (
        typeof value === "string" &&
        schema.maxLength !== undefined &&
        [...value].length > schema.maxLength
    )
        return false;
    if (typeof value === "number" && schema.minimum !== undefined && value < schema.minimum)
        return false;
    if (Array.isArray(value)) {
        return (
            value.length >= (schema.minItems ?? 0) &&
            (schema.items === undefined ||
                value.every((item) => schema.items && matches(schema.items, item)))
        );
    }
    const record = object(value);
    if (!record) return true;
    if (schema.required?.some((key) => !Object.hasOwn(record, key))) return false;
    return Object.entries(record).every(([key, item]) => {
        const child =
            schema.properties && Object.hasOwn(schema.properties, key)
                ? schema.properties[key]
                : undefined;
        return child ? matches(child, item) : schema.additionalProperties !== false;
    });
}

export function argumentsFor(operation: Operation, payload: unknown): object | undefined {
    if (!matches(operation.schema, payload)) throw new Error(`看板参数无效 ${operation.tool}`);
    return operation.method === "POST" ? (payload as object) : undefined;
}
