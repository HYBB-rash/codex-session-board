import { createApp } from "vue";
import ReplyPreview from "./reply-preview.vue";

export interface PreviewSession {
  id: string;
  title: string;
  updatedAt: number;
  hostLabel?: string;
  canOpen?: boolean;
  previewTurnId?: string;
}

export interface PreviewDependencies {
  read: (id: string) => Promise<unknown>;
  openSession: (id: string) => Promise<boolean | undefined>;
  openLink: (url: string) => void;
  changed: () => void;
  canDismiss: () => boolean;
}

export interface PreviewHandle {
  selectedId: string | undefined;
  open: (session: PreviewSession, focus?: boolean) => Promise<void>;
  update: (sessions: PreviewSession[], failed: boolean) => void;
}

declare global {
  interface Window {
    mountReplyPreview: (host: HTMLElement, dependencies: PreviewDependencies) => PreviewHandle;
  }
}

window.mountReplyPreview = (host, dependencies) =>
  createApp(ReplyPreview, { dependencies }).mount(host) as unknown as PreviewHandle;
