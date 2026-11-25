import type { BoardSession } from "./board-view";

const transientNotes = new Map<string, string>();

function draftKey(card: BoardSession) {
    const key = [card.hostId || "local", card.threadId || card.id];
    return `card-note:${JSON.stringify(card.accountScope ? [card.accountScope, ...key] : key)}`;
}

export function writeDraft(card: BoardSession, value: string | null) {
    const key = draftKey(card);
    if (value === null) transientNotes.delete(key);
    else transientNotes.set(key, value);
    try {
        if (value === null) sessionStorage.removeItem(key);
        else sessionStorage.setItem(key, value);
        if (!card.accountScope && (!card.remote || card.id.startsWith("remote:")))
            sessionStorage.removeItem(`failed-note:${card.id}`);
        return true;
    } catch {
        return false;
    }
}

export function readDraft(card: BoardSession) {
    const key = draftKey(card);
    const transient = transientNotes.get(key);
    if (transient !== undefined) return transient;
    try {
        const stored = sessionStorage.getItem(key);
        if (stored !== null || card.accountScope || (card.remote && !card.id.startsWith("remote:")))
            return stored;
        const legacy = sessionStorage.getItem(`failed-note:${card.id}`);
        if (legacy !== null) writeDraft(card, legacy);
        return legacy;
    } catch {
        return null;
    }
}
