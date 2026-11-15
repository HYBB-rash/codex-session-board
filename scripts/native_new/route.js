function remoteDraftRoute(url) {
  const host = url.searchParams.get("hostId");
  const project = url.searchParams.get("projectId");
  if (
    url.protocol !== "codex:" ||
    url.hostname !== "threads" ||
    url.pathname !== "/new-remote" ||
    url.username || url.password || url.port || url.hash ||
    url.searchParams.size !== 2 ||
    url.searchParams.getAll("hostId").length !== 1 ||
    url.searchParams.getAll("projectId").length !== 1 ||
    !host?.trim() || host === "local" || !project?.trim() ||
    [...url.searchParams.keys()].some(key => !["hostId", "projectId"].includes(key))
  ) return null;
  return { kind: "remoteNewThread", hostId: host, projectId: project };
}
