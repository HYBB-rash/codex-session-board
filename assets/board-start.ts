import "./board-view";
import { mountBoardArchive, type AutoArchiveHealth } from "./board-archive";
import { mountBoardBrowse, type CatalogProject, type HostCatalog, type HostIdentity } from "./board-browse";
import { mountBoardSessionReader } from "./board-session-reader";
import { mountMoveFeedback } from "./board-move-feedback";
import { mountBoardEdits } from "./board-edits";
import { mountBoardRefresh } from "./board-refresh";
import { mountProjectAssignment } from "./board-project-assignment";
import { createColumnSortSave, type SortPreference } from "./board-sorting";
import type { BoardSession } from "./board-view";

export interface BoardSnapshot {
  states: Record<string, string>;
  sessions: BoardSession[];
  health: AutoArchiveHealth & {
    sshAliasSetup?: boolean;
    remoteError?: string;
    remoteConnections?: (HostIdentity & { catalog?: HostCatalog })[];
    remoteConnectionErrors?: HostIdentity[];
    remoteRegistry?: HostIdentity[];
  };
  projectCatalog: { loaded: boolean; items: CatalogProject[]; error?: string; remoteHosts?: HostIdentity[] };
  legacyMode?: boolean;
  pendingDrafts?: { project: string; requestId: string }[];
  columnSorts?: Record<string, SortPreference>;
}

declare global {
  interface Window {
    boardRequest?: (path: string, body?: object) => Promise<unknown>;
    createBoardRequest: typeof createBoardRequest;
    startBoard: typeof startBoard;
    mountBoardArchive: typeof mountBoardArchive;
    mountBoardBrowse: typeof mountBoardBrowse;
    mountBoardSessionReader: typeof mountBoardSessionReader;
    mountMoveFeedback: typeof mountMoveFeedback;
    mountBoardEdits: typeof mountBoardEdits;
    mountBoardRefresh: typeof mountBoardRefresh;
    mountProjectAssignment: typeof mountProjectAssignment;
    createColumnSortSave: typeof createColumnSortSave;
  }
}

function createBoardRequest(csrf: string, beforeWrite: () => void) {
  return async (path: string, body?: object): Promise<unknown> => {
    if (body !== undefined) beforeWrite();
    if (window.boardRequest) return window.boardRequest(path, body);
    const token = import.meta.env.DEV && body !== undefined ? await developmentToken() : csrf;
    const options = body === undefined ? undefined : {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-CSRF-Token": token },
      body: JSON.stringify(body),
    };
    const response = await fetch(path, options);
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || "操作失败");
    return result;
  };
}

async function developmentToken(): Promise<string> {
  const response = await fetch("/api/info", { cache: "no-store", redirect: "error" });
  if (!response.ok) throw new Error("无法读取隔离开发会话令牌");
  const { requestToken } = await response.json();
  if (typeof requestToken !== "string" || !requestToken) throw new Error("隔离开发会话令牌缺失");
  return requestToken;
}

function startBoard(load: () => Promise<void>, restore: () => Promise<void> | void) {
  void load().then(restore);
  window.setInterval(load, 4000);
}

window.createBoardRequest = createBoardRequest;
window.startBoard = startBoard;
window.mountBoardArchive = mountBoardArchive;
window.mountBoardBrowse = mountBoardBrowse;
window.mountBoardSessionReader = mountBoardSessionReader;
window.mountMoveFeedback = mountMoveFeedback;
window.mountBoardEdits = mountBoardEdits;
window.mountBoardRefresh = mountBoardRefresh;
window.mountProjectAssignment = mountProjectAssignment;
window.createColumnSortSave = createColumnSortSave;
