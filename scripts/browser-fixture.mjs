export const boardScriptTag = '<script id="reply-preview-code">';

export function beforeBoardScript(html, source) {
  if (!html.includes(boardScriptTag)) throw new Error("缺少看板脚本入口");
  return html.replace(boardScriptTag, () => `<script>${source.replaceAll("</script", "<\\/script")}</script>\n${boardScriptTag}`);
}

export const boardFixtureSource = `window.__boardFixture = bindings => {
  Object.defineProperties(window, Object.getOwnPropertyDescriptors(bindings));
};`;

export function fixtureBoardHtml(html) {
  return beforeBoardScript(html, boardFixtureSource);
}

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
