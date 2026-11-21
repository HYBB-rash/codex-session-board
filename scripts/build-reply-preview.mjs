import fs from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

export const boardScript = /<script id="reply-preview-code">[\s\S]*?<\/script>/;
export const boardStyles = /<style id="board-component-styles">[\s\S]*?<\/style>/;

export async function loadBoardVite(root, temporary) {
  const configPath = path.join(temporary, "board-vite.config.mjs");
  await fs.copyFile(path.join(root, "assets/board-vite.config.mjs"), configPath);
  const { default: boardViteConfig } = await import(pathToFileURL(configPath));
  const vite = await import(pathToFileURL(path.join(temporary, "node_modules/vite/dist/node/index.js")));
  return { vite, config: { ...boardViteConfig(), configFile: false, publicDir: false } };
}

export async function buildReplyPreview(root, temporary, check) {
  for (const file of await fs.readdir(path.join(root, "assets"))) {
    if (/^(reply-|board-).*\.(ts|tsx|vue|css|json)$/.test(file))
      await fs.copyFile(path.join(root, "assets", file), path.join(temporary, file));
  }
  const require = createRequire(path.join(temporary, "package.json"));
  execFileSync(process.execPath, [require.resolve("vue-tsc/bin/vue-tsc.js"),
    "--project", "reply-preview.tsconfig.json"], { cwd: temporary, stdio: "inherit" });
  const { vite, config } = await loadBoardVite(root, temporary);
  const licenseName = "reply-preview.js.LEGAL.txt";
  const result = await vite.build({
    ...config,
    root: temporary,
    define: { ...config.define, "process.env.NODE_ENV": JSON.stringify("production") },
    build: {
      write: false, minify: "esbuild", modulePreload: false, target: "es2022",
      license: { fileName: licenseName },
      lib: { entry: "board-start.ts", name: "CodexSessionBoard", formats: ["iife"] },
      rollupOptions: { output: { inlineDynamicImports: true } },
    },
  });
  const output = [result].flat().flatMap(build => build.output);
  const chunk = output.find(file => file.type === "chunk");
  const notice = output.find(file => file.type === "asset" && file.fileName === licenseName);
  const style = output.find(file => file.type === "asset" && file.fileName.endsWith(".css"));
  if (output.length !== (style ? 3 : 2) || !chunk || !notice || chunk.imports.length || chunk.dynamicImports.length)
    throw new Error("看板必须构建为独立的单页脚本");
  const bundle = chunk.code.trim();
  const css = (style ? String(style.source).trim() : "").replace(/<\/style/gi, "<\\/style");
  const htmlPath = path.join(root, "index.html");
  const html = await fs.readFile(htmlPath, "utf8");
  if (!boardScript.test(html) || !boardStyles.test(html)) throw new Error("缺少看板产物位置");
  const generated = html.replace(boardScript, () =>
    `<script id="reply-preview-code">\n${bundle.replaceAll("</script", "<\\/script")}\n</script>`)
    .replace(boardStyles, () => `<style id="board-component-styles">${css ? `\n${css}\n` : ""}</style>`);
  if (check && generated !== html) throw new Error("回复预览产物与源码不一致");
  if (!check) await fs.writeFile(htmlPath, generated);
  const license = Buffer.from(notice.source);
  const licensePath = path.join(root, "assets", licenseName);
  if (check && !license.equals(await fs.readFile(licensePath))) throw new Error("回复预览许可证与构建不一致");
  if (!check) await fs.writeFile(licensePath, license);
  const vueLicense = await fs.readFile(path.join(path.dirname(require.resolve("vue/package.json")), "LICENSE"));
  const vueLicensePath = path.join(root, "assets/reply-preview.LICENSE.txt");
  if (check && !vueLicense.equals(await fs.readFile(vueLicensePath))) throw new Error("Vue 许可证与锁定依赖不一致");
  if (!check) await fs.writeFile(vueLicensePath, vueLicense);
  return { bytes: Buffer.byteLength(bundle), sha256: createHash("sha256").update(bundle).digest("hex") };
}
