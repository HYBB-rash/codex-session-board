import type { PreviewSession } from "./preview/mount";

export interface BoardSession extends PreviewSession {
    accountScope?: string;
    state: string | null;
    note: string;
    threadId?: string;
    version: number;
    inputVersion: number;
    sectionVersion: string | null;
    readOnly?: boolean;
    manual?: boolean;
    reason?: string;
    placementReason?: string;
    remote?: boolean;
    archived?: boolean;
    canArchive?: boolean;
    archivePending?: boolean;
    presence?: string;
    catalogCurrent?: boolean;
    ordinary?: boolean;
    hostId?: string;
    projectId?: string | null;
    projectIssue?: string | null;
    cwd?: string;
    source?: string;
    followUp?: boolean;
    observationError?: string;
    observationWarning?: string;
    titleSyncPending?: boolean;
    titleSyncError?: string;
}
