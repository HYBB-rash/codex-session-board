import { createApp } from "vue";
import type { BoardSession } from "../../../board-view";
import CardEditor from "./SessionEditor.vue";

export interface EditorPorts {
    save: (session: BoardSession, state: string | null, note: string) => Promise<void>;
    refresh: (session: BoardSession) => Promise<BoardSession>;
    closed: (session: BoardSession) => void;
}

interface EditorHandle {
    open: (session: BoardSession, states: Record<string, string>, state?: string) => void;
    unmount(): void;
}

export function mountCardEditor(host: HTMLElement, ports: EditorPorts): EditorHandle {
    const app = createApp(CardEditor, { ports });
    const editor = app.mount(host) as unknown as Omit<EditorHandle, "unmount">;
    return { open: editor.open, unmount: () => app.unmount() };
}
