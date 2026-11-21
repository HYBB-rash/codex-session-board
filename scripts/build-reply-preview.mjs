import fs from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

export async function buildReplyPreview(root, temporary, check) {
  for (const file of await fs.readdir(path.join(root, "assets"))) {
    if (/^reply-.*\.(ts|tsx|vue|json)$/.test(file))
      await fs.copyFile(path.join(root, "assets", file), path.join(temporary, file));
  }
  const require = createRequire(path.join(temporary, "package.json"));
  execFileSync(process.execPath, [require.resolve("vue-tsc/bin/vue-tsc.js"),
    "--project", "reply-preview.tsconfig.json"], { cwd: temporary, stdio: "inherit" });
  const { parse, compileScript } = require("vue/compiler-sfc");
  const { build } = require("esbuild");
  await build({
    absWorkingDir: temporary, entryPoints: ["reply-preview.ts"], outfile: "reply-preview.js",
    bundle: true, minify: true, format: "iife", legalComments: "external",
    jsx: "automatic", jsxImportSource: "vue",
    alias: { vue: require.resolve("vue/dist/vue.runtime.esm-bundler.js"),
      "vue/jsx-runtime": require.resolve("vue/jsx-runtime") },
    define: { "process.env.NODE_ENV": '"production"', __VUE_OPTIONS_API__: "false",
      __VUE_PROD_DEVTOOLS__: "false", __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false" },
    plugins: [{ name: "reply-vue", setup(builder) {
      builder.onLoad({ filter: /\.vue$/ }, async ({ path: filename }) => {
        const { descriptor, errors } = parse(await fs.readFile(filename, "utf8"), { filename });
        if (errors.length) throw errors[0];
        return { contents: compileScript(descriptor,
          { id: "reply-preview", isProd: true, inlineTemplate: true }).content, loader: "ts" };
      });
    } }],
  });
  const bundle = (await fs.readFile(path.join(temporary, "reply-preview.js"), "utf8")).trim();
  const htmlPath = path.join(root, "index.html");
  const html = await fs.readFile(htmlPath, "utf8");
  const marker = /(<script id="reply-preview-code">)[\s\S]*?(<\/script>)/;
  if (!marker.test(html)) throw new Error("缺少回复预览产物位置");
  const generated = html.replace(marker, () =>
    `<script id="reply-preview-code">\n${bundle.replaceAll("</script", "<\\/script")}\n</script>`);
  if (check && generated !== html) throw new Error("回复预览产物与源码不一致");
  if (!check) await fs.writeFile(htmlPath, generated);
  const license = await fs.readFile(path.join(temporary, "reply-preview.js.LEGAL.txt"));
  const licensePath = path.join(root, "assets/reply-preview.js.LEGAL.txt");
  if (check && !license.equals(await fs.readFile(licensePath))) throw new Error("回复预览许可证与构建不一致");
  if (!check) await fs.writeFile(licensePath, license);
  const vueLicense = await fs.readFile(path.join(path.dirname(require.resolve("vue/package.json")), "LICENSE"));
  const vueLicensePath = path.join(root, "assets/reply-preview.LICENSE.txt");
  if (check && !vueLicense.equals(await fs.readFile(vueLicensePath))) throw new Error("Vue 许可证与锁定依赖不一致");
  if (!check) await fs.writeFile(vueLicensePath, vueLicense);
  return { bytes: Buffer.byteLength(bundle), sha256: createHash("sha256").update(bundle).digest("hex") };
}
