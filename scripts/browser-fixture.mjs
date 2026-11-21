export const fixtureLoadExpression = (revision, startedLoad = "load()") => `(async () => {
  await ${startedLoad};
  await new Promise(resolve => {
    const applied = () => {
      if (data.fixtureRevision === ${revision}) resolve();
      else requestAnimationFrame(applied);
    };
    applied();
  });
})()`;

export function loadFixture(page, fixture) {
  fixture.fixtureRevision = (fixture.fixtureRevision || 0) + 1;
  return page.evaluate(fixtureLoadExpression(fixture.fixtureRevision));
}
