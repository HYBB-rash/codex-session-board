function remoteDraftRoute(url) {
  const host = url.searchParams.get("hostId");
  const project = url.searchParams.get("projectId");
  const target = url.pathname === "/new-target";
  const operation = url.searchParams.get("operationId");
  const keys = target ? ["hostId", "projectId", "operationId"] : ["hostId", "projectId"];
  if (
    url.protocol !== "codex:" ||
    url.hostname !== "threads" ||
    !["/new-remote", "/new-target"].includes(url.pathname) ||
    url.username || url.password || url.port || url.hash ||
    url.searchParams.size !== keys.length ||
    url.searchParams.getAll("hostId").length !== 1 ||
    url.searchParams.getAll("projectId").length !== 1 ||
    !host?.trim() || host === "local" || !project?.trim() ||
    [...url.searchParams.keys()].some(key => !keys.includes(key)) ||
    (target && (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(operation ?? "") ||
      url.searchParams.getAll("operationId").length !== 1))
  ) return null;
  return target
    ? { kind: "targetNewThread", hostId: host, projectId: project, operationId: operation }
    : { kind: "remoteNewThread", hostId: host, projectId: project };
}
