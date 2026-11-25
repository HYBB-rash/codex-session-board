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

function isFields<T extends object>(value: unknown): value is T {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requiredString(value: unknown, label: string): string {
    if (typeof value !== "string" || value.length === 0) {
        throw new Error(`sidebar item ${label} must be a non-empty string`);
    }
    return value;
}

interface RawSidebarPage {
    title: unknown;
    body: unknown;
}

interface RawSidebarItem {
    id: unknown;
    title: unknown;
    route: unknown;
    icon?: unknown;
    page: unknown;
}

interface RawUiState {
    items: unknown;
    autoOpen?: unknown;
}

export function sidebarItem(value: unknown): SidebarItemRegistration {
    if (!isFields<RawSidebarItem>(value)) throw new Error("sidebar item must be an object");
    const id = requiredString(value.id, "id");
    if (!ID_PATTERN.test(id)) throw new Error(`sidebar item id is invalid: ${id}`);
    const title = requiredString(value.title, "title");
    const route = requiredString(value.route, "route");
    if (!route.startsWith("/") || route.startsWith("//") || /\s/.test(route)) {
        throw new Error(`sidebar item route is invalid: ${route}`);
    }
    if (value.icon !== undefined && typeof value.icon !== "string") {
        throw new Error("sidebar item icon must be a string");
    }
    if (!isFields<RawSidebarPage>(value.page))
        throw new Error("sidebar item page must be an object");
    return {
        id,
        title,
        route,
        icon: typeof value.icon === "string" ? value.icon : null,
        page: {
            title: requiredString(value.page.title, "title"),
            body: requiredString(value.page.body, "body"),
        },
    };
}

export function parseUiPayload(value: unknown): UiState {
    if (!isFields<RawUiState>(value) || !Array.isArray(value.items)) {
        throw new Error("ui state is incomplete");
    }
    return {
        items: value.items.map((item) => sidebarItem(item)),
        autoOpen: value.autoOpen === true,
    };
}

export type SidebarRegistry = ReturnType<typeof createSidebarRegistry>;

export function createSidebarRegistry() {
    const items = new Map<string, OwnedSidebarItem>();

    function register(extensionId: string, value: unknown): () => void {
        const item = sidebarItem(value);
        if (items.has(item.id)) {
            throw new Error(`sidebar item id already registered: ${item.id}`);
        }
        items.set(item.id, { ...item, extensionId });
        return () => {
            const current = items.get(item.id);
            if (current?.extensionId === extensionId) items.delete(item.id);
        };
    }

    return { register, list: () => [...items.values()] };
}
