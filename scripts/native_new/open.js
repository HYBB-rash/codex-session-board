async function openRemoteDraft({
  route, projectsManager, selectProject, selectHost, announce, navigate,
}) {
  if (!projectsManager) throw Error("Remote project navigation is unavailable");
  const matches = projectsManager.getRemoteProjects().filter(
    project => project.id === route.projectId,
  );
  if (matches.length !== 1 || matches[0].hostId !== route.hostId)
    throw Error("The remote project is unavailable");
  const project = matches[0];
  if (typeof project.remotePath !== "string" || !project.remotePath.startsWith("/") ||
    project.remotePath.includes("\0")) throw Error("The remote project has an invalid root");
  const selection = { type: "remote", projectId: project.id };
  await selectProject(selection);
  selectHost(project.hostId);
  announce();
  navigate(selection);
}
