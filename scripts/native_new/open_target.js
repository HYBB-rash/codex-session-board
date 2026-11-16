async function openTargetDraft({ route, navigate }) {
  await navigate({
    nativeTargetDraft: {
      hostId: route.hostId, projectId: route.projectId, operationId: route.operationId,
    },
    focusComposerNonce: Date.now(),
  });
}
