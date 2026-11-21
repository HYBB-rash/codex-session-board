import { createApp } from "vue";
import CardEditor from "./board-editor.vue";
import type { BoardSession } from "./board-view";

export type EditorSession = BoardSession & {
  threadId?: string;
  note: string;
  version: number;
  inputVersion: number;
};

export interface EditorPorts {
  save: (session: EditorSession, state: string | null, note: string) => Promise<void>;
  refresh: (session: EditorSession) => Promise<EditorSession>;
  closed: (session: EditorSession) => void;
}

interface EditorHandle {
  open: (session: EditorSession, states: Record<string, string>, state?: string) => void;
}

declare global {
  interface Window {
    mountCardEditor: (host: HTMLElement, ports: EditorPorts) => EditorHandle;
  }
}

window.mountCardEditor = (host, ports) => createApp(CardEditor, { ports }).mount(host) as unknown as EditorHandle;
