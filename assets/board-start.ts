import "./reply-preview";
import type { BoardSession } from "./board-view";

export interface BoardSnapshot {
  states: Record<string, string>;
  sessions: (BoardSession & { version: number; inputVersion: number; sectionVersion: string | null; readOnly?: boolean })[];
  health: { sshAliasSetup?: boolean };
  projectCatalog: { loaded: boolean; items: { id: string; paths: string[] }[] };
  legacyMode?: boolean;
  pendingDrafts?: { project: string; requestId: string }[];
}

interface SnapshotPorts {
  current(): number;
  locked(): boolean;
  request(path: "/api/sessions"): Promise<BoardSnapshot>;
  received(snapshot: BoardSnapshot): void;
  failed(message: string): void;
}

declare global {
  interface Window {
    boardRequest?: (path: string, body?: object) => Promise<unknown>;
    createBoardRequest: typeof createBoardRequest;
    readBoardSnapshot: typeof readBoardSnapshot;
    startBoard: typeof startBoard;
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

async function readBoardSnapshot(serial: number, ports: SnapshotPorts): Promise<void> {
  try {
    const snapshot = await ports.request("/api/sessions");
    if (serial === ports.current() && !ports.locked()) ports.received(snapshot);
  } catch (error) {
    if (serial === ports.current()) ports.failed(error instanceof Error ? error.message : String(error));
  }
}

function startBoard(load: () => Promise<void>, restore: () => Promise<void> | void) {
  void load().then(restore);
  window.setInterval(load, 4000);
}

window.createBoardRequest = createBoardRequest;
window.readBoardSnapshot = readBoardSnapshot;
window.startBoard = startBoard;
