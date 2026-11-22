import type { BoardLedger } from "./archive.js";

export interface DraftRecord {
    project: string;
    requestId: string;
    stage: "starting" | "created" | "opened" | "abandoned";
    threadId: string | null;
    source: "legacy" | "native";
}

export interface DraftInput {
    projectId?: string;
    hostId?: string;
    requestId?: string;
    operationId?: string;
    confirmed?: true;
}

export function unresolvedDrafts(ledger: BoardLedger): DraftRecord[] {
    return (ledger.pendingDrafts ?? []).filter(
        ({ stage }) => stage === "starting" || stage === "created",
    );
}

export function stageDraft(ledger: BoardLedger, draft: DraftRecord): BoardLedger {
    const pending = ledger.pendingDrafts ?? [];
    const previous = pending.find(({ requestId }) => requestId === draft.requestId);
    if (previous && (previous.project !== draft.project || previous.source !== draft.source)) {
        throw new Error("新建请求已绑定另一个项目");
    }
    return {
        ...ledger,
        pendingDrafts: [draft, ...pending.filter(({ requestId }) => requestId !== draft.requestId)],
    };
}
