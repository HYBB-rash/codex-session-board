export interface SidebarPageContent {
    title: string;
    body: string;
}

export interface SidebarItemRegistration {
    id: string;
    title: string;
    route: string;
    icon: string | null;
    page: SidebarPageContent;
}

export interface OwnedSidebarItem extends SidebarItemRegistration {
    extensionId: string;
}

export interface UiState {
    items: SidebarItemRegistration[];
    autoOpen: boolean;
}

const ID_PATTERN = /^[a-z0-9][a-z0-9-]{0,63}$/;

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requiredString(value: Record<string, unknown>, key: string): string {
    const item = value[key];
    if (typeof item !== "string" || item.length === 0) {
        throw new Error(`sidebar item ${key} must be a non-empty string`);
    }
    return item;
}

export function sidebarItem(value: unknown): SidebarItemRegistration {
    if (!isRecord(value)) throw new Error("sidebar item must be an object");
    const id = requiredString(value, "id");
    if (!ID_PATTERN.test(id)) throw new Error(`sidebar item id is invalid: ${id}`);
    const title = requiredString(value, "title");
    const route = requiredString(value, "route");
    if (!route.startsWith("/") || route.startsWith("//") || /\s/.test(route)) {
        throw new Error(`sidebar item route is invalid: ${route}`);
    }
    const icon = value["icon"];
    if (icon !== undefined && typeof icon !== "string") {
        throw new Error("sidebar item icon must be a string");
    }
    const page = value["page"];
    if (!isRecord(page)) throw new Error("sidebar item page must be an object");
    return {
        id,
        title,
        route,
        icon: typeof icon === "string" ? icon : null,
        page: {
            title: requiredString(page, "title"),
            body: requiredString(page, "body"),
        },
    };
}

export function parseUiPayload(value: unknown): UiState {
    if (!isRecord(value)) throw new Error("ui state is incomplete");
    const items = value["items"];
    if (!Array.isArray(items)) throw new Error("ui state is incomplete");
    return {
        items: items.map((item) => sidebarItem(item)),
        autoOpen: value["autoOpen"] === true,
    };
}

export class SidebarRegistry {
    private readonly items = new Map<string, OwnedSidebarItem>();

    register(extensionId: string, value: unknown): () => void {
        const item = sidebarItem(value);
        if (this.items.has(item.id)) {
            throw new Error(`sidebar item id already registered: ${item.id}`);
        }
        this.items.set(item.id, { ...item, extensionId });
        return () => {
            const current = this.items.get(item.id);
            if (current?.extensionId === extensionId) this.items.delete(item.id);
        };
    }

    list(): OwnedSidebarItem[] {
        return [...this.items.values()];
    }
}
