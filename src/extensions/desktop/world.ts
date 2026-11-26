/// <reference lib="dom" />

export interface SectionMove {
    kind: "move";
    accountId: string;
    userId: string;
    hostId: string;
    threadId: string;
    from: string | null;
    to: string;
}

export type DesktopCommand =
    | { kind: "context" }
    | SectionMove
    | (DesktopAccount &
          (
              | { kind: "open"; hostId: string; threadId: string; besideBoard?: boolean }
              | { kind: "new"; hostId: string; projectId: string | null; roots: string[] }
              | { kind: "draft"; draftId: string }
              | { kind: "data-home"; hostId: string }
              | { kind: "settings" }
          ));

export type DesktopRequest =
    | DesktopCommand
    | { kind: "local-project-root"; cwd: string; roots: string[] };

interface DesktopAccount {
    accountId: string;
    userId: string;
}

interface DesktopSection {
    id: string;
    name: string;
    hostSectionIds?: Record<string, string>;
    itemKeys: string[];
}

interface DesktopScope {
    get<T>(atom: DesktopAtom<T>, key?: string): T;
    watch?(observer: (reader: Pick<DesktopScope, "get">) => void): () => void;
    set: unknown;
    scope?: { __scopeBrand?: string };
}

interface DesktopAtom<T> {
    readonly value: T;
}

interface Fiber {
    return?: Fiber;
    type?: unknown;
    memoizedState?: Hook;
    memoizedProps?: { value?: { router?: DesktopRouter } };
}

interface DesktopRouter {
    navigate(path: string, options?: { state?: object }): Promise<unknown>;
}

type LocalProjects = Record<string, { rootPaths: string[] }>;

interface WorkspaceModule {
    Gv?: DesktopAtom<LocalProjects>;
    Gt?: DesktopAtom<{ type: string; projectId: string } | null>;
    n8?: DesktopAtom<{ id: string; hostId: string; remotePath: string }[]>;
}

interface DraftModule {
    RH?: DesktopAtom<string>;
    Tl?: DesktopAtom<string | null>;
}

interface ObservedDraft {
    id: string;
    scope: DesktopScope;
    result: Promise<string | null>;
    cancel(): void;
}

interface ComposerModule {
    startNewConversation?(
        scope: DesktopScope,
        options: {
            activeProject: { projectId: string; projectKind: "local" | "remote" } | null;
            codexAppMode: "codex";
            freshDraft: true;
            prefillComposerMode: "local";
            prefillRemoteExecutionHostId?: string;
        },
    ): unknown;
}

interface HostMessageModule {
    O1?: {
        dispatchHostMessage(message: {
            type: "remote-workspace-root-requested";
            mode: "add";
            hostId: string;
            host: string;
            initialPath: string;
            setActive: true;
        }): void;
    };
}

interface Hook {
    next?: Hook;
    memoizedState?: { current?: DesktopScope };
}

interface DesktopModules {
    (key: string): Record<string, unknown>;
    m: Record<string, { toString(): string }>;
}

interface AuthModule {
    chatGptAuthenticatedAccountId$?: DesktopAtom<string | null>;
    chatGptAuthenticatedUserId$?: DesktopAtom<string | null>;
}

interface SidebarModule {
    QV?: DesktopAtom<DesktopSection[]>;
    f2?(
        scope: DesktopScope,
        sectionId: string,
        itemKey: string,
        beforeItem: null,
        options: { threadHostId: string },
    ): unknown;
}

interface RemoteConnectionsModule {
    u8?: DesktopAtom<{ hostId: string; displayName?: string }[] | undefined>;
}

function shellFibers(): Fiber | undefined {
    const shell = document.querySelector("[data-app-shell-workspace]");
    const key = shell && Object.keys(shell).find((name) => name.startsWith("__reactFiber$"));
    return shell && key ? (Reflect.get(shell, key) as Fiber | undefined) : undefined;
}

function appScope(): DesktopScope {
    for (let current = shellFibers(); current; current = current.return) {
        if (typeof current.type !== "function") continue;
        for (let hook = current.memoizedState; hook; hook = hook.next) {
            const scope = hook.memoizedState?.current;
            if (
                scope?.scope?.__scopeBrand === "AppScope" &&
                typeof scope.get === "function" &&
                typeof scope.set === "function"
            )
                return scope;
        }
    }
    throw new Error("Codex desktop application context is unavailable");
}

function desktopRouter(): DesktopRouter {
    for (let current = shellFibers(); current; current = current.return) {
        const router = current.memoizedProps?.value?.router;
        if (typeof router?.navigate === "function") return router;
    }
    throw new Error("Codex navigation is unavailable");
}

export function resolveLocalProjectId(projects: LocalProjects, projectId: string, roots: string[]) {
    const canonical = projects[projectId]?.rootPaths[0];
    if (canonical && roots.includes(canonical)) return projectId;
    if (roots.length === 0) return undefined;
    const matches = Object.entries(projects).filter(
        ([, { rootPaths }]) =>
            rootPaths.length === roots.length &&
            rootPaths.every((root, index) => root === roots[index]),
    );
    if (matches.length > 1)
        throw new Error("Codex 中有多个本机项目使用相同目录，无法确定要打开的项目");
    return matches[0]?.[0];
}

function workspaceModule(modules: DesktopModules): WorkspaceModule {
    return exported(modules, [
        "THREAD_PROJECT_ASSIGNMENTS",
        "LOCAL_PROJECTS",
        "REMOTE_PROJECTS",
        "n8:()=>",
    ]);
}

function composerProject(
    command: Extract<DesktopCommand, { kind: "new" }>,
    workspace: WorkspaceModule,
    scope: DesktopScope,
) {
    if (command.projectId === null && command.hostId === "local") return null;
    const { hostId, projectId, roots } = command;
    if (hostId === "local" && workspace.Gv && projectId !== null) {
        const nativeId = resolveLocalProjectId(scope.get(workspace.Gv), projectId, roots);
        if (nativeId) return { projectId: nativeId, projectKind: "local" as const };
    }
    if (hostId !== "local" && workspace.n8) {
        const projects = scope
            .get(workspace.n8)
            .filter((entry) => entry.hostId === hostId && roots.includes(entry.remotePath));
        const project =
            projects.find(({ id }) => id === projectId) ??
            (projects.length === 1 ? projects[0] : undefined);
        if (project) return { projectId: project.id, projectKind: "remote" as const };
    }
    return undefined;
}

function registerRemoteProject(
    command: Extract<DesktopCommand, { kind: "new" }>,
    modules: DesktopModules,
): string {
    const initialPath = command.roots[0];
    if (command.hostId === "local" || command.roots.length !== 1 || !initialPath)
        throw new Error("Codex 尚未登记此项目，请在原生项目设置中核对目录");
    const host: HostMessageModule = exported(modules, [
        "dispatchHostMessage(e){",
        "deliverOrBufferMessage",
        "mcp-request-delivery",
    ]);
    if (!host.O1) throw new Error("Codex 原生项目登记入口不可用");
    host.O1.dispatchHostMessage({
        type: "remote-workspace-root-requested",
        mode: "add",
        hostId: command.hostId,
        host: command.hostId,
        initialPath,
        setActive: true,
    });
    return "registration";
}

async function observeDraft(
    scope: DesktopScope,
    drafts: DraftModule,
    selection: WorkspaceModule["Gt"],
    projectId: string,
    previous: string | undefined,
) {
    const { RH, Tl } = drafts;
    if (!scope.watch || !RH || !Tl || !selection) return;
    let id: string;
    for (let attempt = 0; ; attempt++) {
        id = scope.get(RH, "new-conversation");
        const selected = scope.get(selection);
        if (
            id.startsWith("client-new-thread:") &&
            id !== previous &&
            selected?.type === "local" &&
            selected.projectId === projectId
        )
            break;
        if (attempt === 10) return;
        await new Promise<void>((resolve) => window.setTimeout(resolve, 50));
    }

    const key = Symbol.for("codex-enhancer.local-project-draft");
    (Reflect.get(window, key) as ObservedDraft | undefined)?.cancel();
    let stop: (() => void) | undefined;
    let complete: (threadId: string | null) => void = () => {};
    const result = new Promise<string | null>((resolve) => {
        complete = resolve;
    });
    const finish = (threadId: string | null) => {
        complete(threadId);
        queueMicrotask(() => stop?.());
    };
    const cancel = () => finish(null);
    const draft: ObservedDraft = { id, scope, result, cancel };
    Reflect.set(window, key, draft);
    stop = scope.watch(({ get }) => {
        const active = get(selection);
        if (active?.type !== "local" || active.projectId !== projectId) return cancel();
        const threadId = get(Tl, id);
        if (threadId) return finish(threadId);
        if (get(RH, "new-conversation") !== id) cancel();
    });
    return { draftId: id };
}

async function openComposer(
    command: Extract<DesktopCommand, { kind: "new" }>,
    modules: DesktopModules,
    scope: DesktopScope,
) {
    const workspace =
        command.projectId === null && command.hostId === "local" ? {} : workspaceModule(modules);
    const activeProject = composerProject(command, workspace, scope);
    if (activeProject === undefined) return registerRemoteProject(command, modules);
    const composer: ComposerModule = exported(modules, [
        "startNewConversation:",
        "defaultCloudEnvironment",
        "default_new_chats_to_work",
    ]);
    const start = composer.startNewConversation;
    if (!start) throw new Error("当前 Codex 版本未提供原生输入页入口");
    const drafts: DraftModule =
        activeProject?.projectKind === "local"
            ? exported(
                  modules,
                  ["draft-thread-identities-v1", "getThreadScopeRouteId:", "Tl:()=>"],
                  false,
              )
            : {};
    const previous = drafts.RH && scope.get(drafts.RH, "new-conversation");
    await start(scope, {
        activeProject,
        codexAppMode: "codex",
        freshDraft: true,
        prefillComposerMode: "local",
        ...(command.hostId === "local" ? {} : { prefillRemoteExecutionHostId: command.hostId }),
    });
    if (activeProject?.projectKind === "local") {
        return (
            (await observeDraft(scope, drafts, workspace.Gt, activeProject.projectId, previous)) ??
            "composer"
        );
    }
    return "composer";
}

async function navigate(
    command: Exclude<DesktopCommand, { kind: "context" | "move" | "draft" | "data-home" }>,
    modules: DesktopModules,
    scope: DesktopScope,
) {
    if (command.kind === "settings") {
        await desktopRouter().navigate("/remote-connections");
        return null;
    }
    if (command.kind === "open") {
        if (!command.besideBoard) {
            const row = [
                ...document.querySelectorAll<HTMLElement>("[data-app-action-sidebar-thread-row]"),
            ].find(
                (candidate) =>
                    candidate.getAttribute("data-app-action-sidebar-thread-host-id") ===
                        command.hostId &&
                    candidate.getAttribute("data-app-action-sidebar-thread-id") ===
                        `local:${command.threadId}`,
            );
            if (row) {
                row.click();
                return null;
            }
        }
        const query = new URLSearchParams({ hostId: command.hostId });
        await desktopRouter().navigate(`/local/${encodeURIComponent(command.threadId)}?${query}`);
        return null;
    }
    return openComposer(command, modules, scope);
}

async function loadedModules(): Promise<DesktopModules> {
    const scripts = document.querySelectorAll<HTMLScriptElement>("script[type=module][src]");
    for (const script of scripts) {
        const url = new URL(script.src);
        if (url.origin !== location.origin || !url.pathname.startsWith("/assets/")) continue;
        const module = (await import(url.href)) as { __webpack_require__?: DesktopModules };
        if (module.__webpack_require__?.m) return module.__webpack_require__;
    }
    throw new Error("Codex desktop module loader is unavailable");
}

function exported(
    modules: DesktopModules,
    markers: string[],
    required = true,
): Record<string, unknown> {
    const keys = Object.keys(modules.m).filter((key) => {
        const source = modules.m[key]?.toString() ?? "";
        return markers.every((marker) => source.includes(marker));
    });
    if (keys.length !== 1 || keys[0] === undefined) {
        if (!required) return {};
        throw new Error("This desktop build does not expose the required sidebar operation");
    }
    return modules(keys[0]);
}

function remoteHostNames(modules: DesktopModules, scope: DesktopScope): Record<string, string> {
    const connections: RemoteConnectionsModule = exported(
        modules,
        ["remoteSshConnections:", "remoteWslConnections:", "oneToOnePairingInAppEnabled:", "u8:"],
        false,
    );
    return Object.fromEntries(
        (connections.u8 ? (scope.get(connections.u8) ?? []) : []).flatMap(
            ({ hostId, displayName }) => {
                const name = displayName?.trim();
                return name ? [[hostId, name]] : [];
            },
        ),
    );
}

function desktopAccount(modules: DesktopModules, scope: DesktopScope) {
    const auth: AuthModule = exported(modules, [
        "chatGptAuthenticatedAccountId$:()=>",
        "chatGptAuthenticatedUserId$:()=>",
    ]);
    const account = auth.chatGptAuthenticatedAccountId$;
    const user = auth.chatGptAuthenticatedUserId$;
    if (!account || !user) throw new Error("Codex account capabilities are unavailable");
    const accountId = scope.get(account);
    const userId = scope.get(user);
    if (!accountId || !userId) throw new Error("Codex sign-in is required");
    return { accountId, userId };
}

function sidebarContext(modules: DesktopModules, scope: DesktopScope) {
    const sidebar: SidebarModule = exported(
        modules,
        ["sidebar-accountless-section-order-v1", "threadHostId", "wasPinned"],
        false,
    );
    const current = sidebar.QV && scope.get(sidebar.QV);
    return { sections: Array.isArray(current) ? current : [], move: sidebar.f2 };
}

async function executionDataHome(modules: DesktopModules, hostId: string) {
    const paths = exported(modules, [
        "executionPaths.readHomePaths",
        "Home directory is unavailable",
    ]);
    const readers = Object.values(paths).filter(
        (value): value is (id: string) => Promise<{ codexHome?: string }> =>
            typeof value === "function" &&
            /appServices\.executionPaths\.readHomePaths\(/.test(
                Function.prototype.toString.call(value),
            ),
    );
    if (readers.length !== 1) throw new Error("当前 Codex 版本的主机数据路径接口不可用");
    const result = await readers[0]?.(hostId);
    if (typeof result?.codexHome !== "string" || !result.codexHome.startsWith("/"))
        throw new Error("目标主机未提供可用的 Codex 数据目录");
    return result.codexHome;
}

async function boundDraft(scope: DesktopScope, draftId: string) {
    const key = Symbol.for("codex-enhancer.local-project-draft");
    const draft = Reflect.get(window, key) as ObservedDraft | undefined;
    if (draft?.id !== draftId || draft.scope !== scope) return null;
    try {
        return await draft.result;
    } finally {
        if (Reflect.get(window, key) === draft) Reflect.deleteProperty(window, key);
    }
}

async function desktopWorld(command: DesktopCommand): Promise<unknown> {
    if (
        window.top !== window ||
        location.origin !== "app://-" ||
        location.pathname !== "/index.html"
    ) {
        throw new Error("Codex desktop window is unavailable");
    }
    const modules = await loadedModules();
    const scope = appScope();
    const { accountId, userId } = desktopAccount(modules, scope);
    if (command.kind === "context")
        return {
            accountId,
            userId,
            sections: sidebarContext(modules, scope).sections,
            remoteHostNames: remoteHostNames(modules, scope),
        };
    if (accountId !== command.accountId || userId !== command.userId || scope !== appScope()) {
        throw new Error("Codex account retired");
    }
    if (command.kind === "draft") return boundDraft(scope, command.draftId);
    if (command.kind === "data-home") {
        const home = await executionDataHome(modules, command.hostId);
        const current = desktopAccount(modules, appScope());
        if (current.accountId !== accountId || current.userId !== userId)
            throw new Error("Codex account retired");
        return home;
    }
    if (command.kind !== "move") return navigate(command, modules, scope);
    const { sections, move } = sidebarContext(modules, scope);
    const itemKey = `codex:thread:local:${command.threadId}`;
    const ownedKey = `codex:thread:${command.hostId}:${command.threadId}`;
    const sidebarPosition = sections.find((section) => section.itemKeys.includes(ownedKey));
    if (!sections.some((section) => section.id === command.to) || typeof move !== "function") {
        throw new Error("Codex destination section is unavailable");
    }
    if (sidebarPosition && sidebarPosition.id !== command.from)
        throw new Error("Sidebar placement changed");
    await move(scope, command.to, itemKey, null, { threadHostId: command.hostId });
    return null;
}

export function desktopWorldSource(command: DesktopCommand): string {
    const functions = [
        shellFibers,
        appScope,
        desktopRouter,
        resolveLocalProjectId,
        workspaceModule,
        composerProject,
        registerRemoteProject,
        observeDraft,
        openComposer,
        navigate,
        loadedModules,
        exported,
        remoteHostNames,
        desktopAccount,
        sidebarContext,
        executionDataHome,
        boundDraft,
        desktopWorld,
    ].map((fn) => fn.toString());
    return `(()=>{${functions.join("\n")}\nreturn desktopWorld(${JSON.stringify(command)});})()`;
}
