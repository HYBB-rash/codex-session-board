import { createApp } from "vue";
import CardEditor from "./board-editor.vue";
import type { BoardSession } from "./board-view";

export interface EditorPorts {
  save: (session: BoardSession, state: string | null, note: string) => Promise<void>;
  refresh: (session: BoardSession) => Promise<BoardSession>;
  closed: (session: BoardSession) => void;
}

interface EditorHandle {
  open: (session: BoardSession, states: Record<string, string>, state?: string) => void;
}

export function mountCardEditor(host: HTMLElement, ports: EditorPorts): EditorHandle {
  return createApp(CardEditor, { ports }).mount(host) as unknown as EditorHandle;
}
