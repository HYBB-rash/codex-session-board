export interface FrameIdentity {
    url: string;
    parent: unknown;
}

export function isAppUrl(url: string): boolean {
    try {
        return new URL(url).protocol === "app:";
    } catch {
        return false;
    }
}

// The main frame is the only frame whose parent is null. A missing parent
// property is rejected so an unexpected Electron shape cannot skip the check.
export function trustedAppFrame(frame: FrameIdentity | null | undefined): boolean {
    if (frame == null || !("parent" in frame) || frame.parent !== null) return false;
    return typeof frame.url === "string" && isAppUrl(frame.url);
}
