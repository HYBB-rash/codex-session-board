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

export function trustedAppFrame(frame: FrameIdentity | null | undefined): boolean {
    if (frame == null || !("parent" in frame) || frame.parent !== null) return false;
    return typeof frame.url === "string" && isAppUrl(frame.url);
}
