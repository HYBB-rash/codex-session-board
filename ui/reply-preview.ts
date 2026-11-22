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
  canDismiss: () => boolean;
  canRead: (id: string) => boolean;
}

export interface PreviewHandle {
  selectedId: string | undefined;
  open: (session: PreviewSession, focus?: boolean) => Promise<void>;
  update: (sessions: PreviewSession[], failed: boolean) => void;
  unmount(): void;
}

export function mountReplyPreview(host: HTMLElement, dependencies: PreviewDependencies): PreviewHandle {
  const app = createApp(ReplyPreview, { dependencies, root: host.getRootNode() as ShadowRoot | Document });
  const preview = app.mount(host) as unknown as Omit<PreviewHandle, "unmount">;
  return {
    get selectedId() { return preview.selectedId; },
    open: preview.open,
    update: preview.update,
    unmount: () => app.unmount(),
  };
}
