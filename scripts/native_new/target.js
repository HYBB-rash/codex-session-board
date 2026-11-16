function targetDraftRuntime({ createId, scopeId, request, existing }) {
  const operations = new Map();
  const drafts = new Map();
  const required = (value, name) => {
    if (typeof value !== "string" || !value.trim() || value.includes("\0"))
      throw Error(`Invalid target ${name}`);
    return value;
  };
  const register = route => {
    const hostId = required(route.hostId, "host");
    const projectId = required(route.projectId, "project");
    const operationId = required(route.operationId, "operation");
    if (hostId === "local" || !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(operationId))
      throw Error("Invalid target draft identity");
    const prior = operations.get(operationId);
    if (prior) {
      if (prior.hostId !== hostId || prior.projectId !== projectId)
        throw Error("The target operation belongs to another project");
      return prior;
    }
    const generated = createId();
    const clientThreadId = generated.slice(0, generated.lastIndexOf(":") + 1) + operationId;
    if (clientThreadId === operationId) throw Error("Unsupported native draft identity");
    const target = { hostId, projectId, operationId, clientThreadId };
    operations.set(operationId, target);
    drafts.set(clientThreadId, target);
    return target;
  };
  const get = (scope, clientThreadId) => drafts.get(
    clientThreadId ?? (typeof scope === "string" ? scope : scopeId(scope)),
  );
  const targetFor = descriptor => {
    const target = operations.get(descriptor.operationId);
    if (!target || ["hostId", "projectId", "clientThreadId"].some(
      key => descriptor[key] !== target[key],
    )) throw Error("The target draft is unavailable");
    return target;
  };
  const resolve = async (scope, descriptor) => {
    const target = targetFor(descriptor);
    const bound = target.conversationId ?? existing(scope, target.clientThreadId);
    if (bound) throw Error(`Target operation already created thread ${bound}; open that thread`);
    if (target.invalid) throw Error(target.invalid);
    const { project } = await request(scope, target.hostId, "project/read", {
      projectId: target.projectId,
    });
    const roots = project?.roots?.map(root => root.path);
    if (project?.id !== target.projectId || !roots?.length || roots.some(
      root => typeof root !== "string" || !root.startsWith("/") || root.includes("\0"),
    ) || new Set(roots).size !== roots.length)
      throw Error("The target project has no valid workspace roots");
    if (target.workspaceRoots && JSON.stringify(target.workspaceRoots) !== JSON.stringify(roots)) {
      target.invalid = "The target project roots changed; open a new target draft";
      throw Error(target.invalid);
    }
    target.workspaceRoots = [...roots];
    target.cwd = roots[0];
    return { cwd: target.cwd, workspaceRoots: [...roots], projectAssignment: undefined,
      projectlessOutputDirectory: null };
  };
  const creationInputs = async (scope, hostId, params, inputs) => {
    const target = targetFor(params.nativeTargetDraft);
    if (hostId !== target.hostId || params.projectAssignment !== undefined)
      throw Error("The target draft host or assignment changed");
    const workspace = await resolve(scope, target);
    if (params.cwd !== workspace.cwd ||
      JSON.stringify(params.workspaceRoots) !== JSON.stringify(workspace.workspaceRoots))
      throw Error("The target draft workspace changed");
    inputs.projectId = target.projectId;
    return inputs;
  };
  const accepted = (descriptor, conversationId) => {
    const target = targetFor(descriptor);
    if (target.conversationId && target.conversationId !== conversationId)
      throw Error(`Target operation already created thread ${target.conversationId}`);
    target.conversationId = conversationId;
  };
  const beforeFirstTurn = async (scope, descriptor) => {
    const target = targetFor(descriptor);
    const { project } = await request(scope, target.hostId, "project/read", {
      projectId: target.projectId,
    });
    if (project?.id !== target.projectId || JSON.stringify(project.roots?.map(root => root.path)) !==
      JSON.stringify(target.workspaceRoots)) {
      target.invalid = `The target project changed before the first turn; thread ${target.conversationId ?? "not started"}`;
      throw Error(target.invalid);
    }
  };
  const verify = async (scope, descriptor, conversationId) => {
    const target = targetFor(descriptor);
    const { thread } = await request(scope, target.hostId, "thread/read", {
      threadId: conversationId, includeTurns: false,
    });
    if (thread?.id !== conversationId || thread.projectId !== target.projectId)
      throw Error(`Created thread ${conversationId} lost its target project; open that thread before retrying`);
  };
  return { register, get, resolve, creationInputs, accepted, beforeFirstTurn, verify };
}
