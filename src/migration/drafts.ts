import type { DraftRecord } from "../extensions/drafts.js";
import type { LegacyAccountBinding, LegacyImportDatabase } from "./projection.js";

function fieldsOf(prefix: string, project: string): unknown[] | null {
    if (!project.startsWith(prefix)) return null;
    try {
        const value: unknown = JSON.parse(project.slice(prefix.length));
        return Array.isArray(value) ? value : null;
    } catch (error) {
        if (!(error instanceof SyntaxError)) throw error;
        return null;
    }
}

function canonicalTarget(
    database: LegacyImportDatabase,
    requestId: string,
    alias: string,
): string | undefined {
    const raw = database.settings[`native_draft_v1:${requestId}`];
    if (!raw) return undefined;
    const record = JSON.parse(raw) as {
        operationId?: unknown;
        projectId?: unknown;
        stage?: unknown;
        target?: { projectId?: unknown };
    } | null;
    return record?.projectId === alias &&
        record.operationId === requestId &&
        (record.stage === "prepared" || record.stage === "opened") &&
        typeof record.target?.projectId === "string"
        ? record.target.projectId
        : undefined;
}

function projectKey(
    database: LegacyImportDatabase,
    binding: LegacyAccountBinding,
    alias: string,
    requestId: string,
): string | undefined {
    const target = fieldsOf("target:", alias);
    const legacy = fieldsOf("remote:", alias);
    const encoded = target ?? legacy;
    if (!encoded) {
        if (alias.startsWith("target:") || alias.startsWith("remote:")) return alias;
        return database.hostId === "local"
            ? alias
            : `target:${JSON.stringify([database.hostId, "targetServer", alias])}`;
    }
    const source = target ? encoded[0] : encoded[1];
    const identifier = encoded[2];
    if (
        encoded.length !== 3 ||
        (legacy && encoded[0] !== "codex") ||
        (target && encoded[1] !== "targetServer" && encoded[1] !== "targetLegacy") ||
        typeof source !== "string" ||
        typeof identifier !== "string" ||
        !Object.hasOwn(binding.hosts, source)
    ) {
        return undefined;
    }
    const host = binding.hosts[source];
    const canonical =
        target?.[1] === "targetLegacy" ? canonicalTarget(database, requestId, alias) : identifier;
    if (!canonical) return alias;
    return host === "local"
        ? canonical
        : `target:${JSON.stringify([host, "targetServer", canonical])}`;
}

export function projectPendingDrafts(
    database: LegacyImportDatabase,
    binding: LegacyAccountBinding,
): DraftRecord[] {
    return Object.entries(database.settings).flatMap(([key, requestId]) => {
        if (!key.startsWith("draft_pending:") || !requestId) return [];
        const raw = database.settings[`draft_request:${requestId}`];
        if (!raw) return [];
        const entry = JSON.parse(raw) as Partial<DraftRecord> | null;
        if (
            entry?.project !== key.slice("draft_pending:".length) ||
            entry.requestId !== requestId ||
            (entry.stage !== "starting" && entry.stage !== "created")
        ) {
            return [];
        }
        const project = projectKey(database, binding, entry.project, requestId);
        return project
            ? [
                  {
                      project,
                      requestId,
                      stage: entry.stage,
                      threadId: typeof entry.threadId === "string" ? entry.threadId : null,
                      source: "legacy",
                  },
              ]
            : [];
    });
}
