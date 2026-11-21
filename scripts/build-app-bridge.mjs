import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { buildReplyPreview } from "./build-reply-preview.mjs";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const check = process.argv.includes("--check");
if (process.argv.slice(2).some(argument => !["--check", "--offline"].includes(argument)))
  throw new Error("用法：node scripts/build-app-bridge.mjs [--check] [--offline]");
const temporary = await fs.mkdtemp(path.join(os.tmpdir(), "board-bridge-build-"));
try {
  for (const [source, destination] of [
    ["app-bridge.package.json", "package.json"],
    ["app-bridge.package-lock.json", "package-lock.json"],
    ["app-bridge-entry.mjs", "app-bridge-entry.mjs"],
  ]) await fs.copyFile(path.join(root, "assets", source), path.join(temporary, destination));
  const operations = JSON.parse(await fs.readFile(path.join(root, "assets/board-operations.json"), "utf8"));
  const routes = { GET: {}, POST: {} };
  const tools = new Set(), actions = new Set();
  for (const {action, method, path, tool} of operations) {
    if (tools.has(tool) || actions.has(action) || Object.hasOwn(routes[method], path))
      throw new Error(`重复的看板操作 ${action} ${method} ${path} ${tool}`);
    tools.add(tool);
    actions.add(action);
    routes[method][path] = tool;
  }
  const previewTool = operations.find(operation => operation.action === "PreviewReply").tool;
  await fs.writeFile(path.join(temporary, "bridge-routes.json"), JSON.stringify({routes, previewTool}));
  execFileSync("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund",
    ...(process.argv.includes("--offline") ? ["--offline"] : [])], { cwd: temporary, stdio: "inherit" });
  execFileSync(process.execPath, [path.join(temporary, "node_modules/esbuild/bin/esbuild"),
    "app-bridge-entry.mjs", "--bundle", "--minify", "--format=iife", "--legal-comments=external",
    "--outfile=app-bridge.js"], { cwd: temporary, stdio: "inherit" });
  const files = ["app-bridge.js", "app-bridge.js.LEGAL.txt"];
  const report = {};
  for (const file of files) {
    const generated = await fs.readFile(path.join(temporary, file));
    if (check) {
      const current = await fs.readFile(path.join(root, "assets", file));
      if (!generated.equals(current)) throw new Error(`桥接产物与源码不一致：${file}`);
    } else await fs.writeFile(path.join(root, "assets", file), generated);
    report[file] = { bytes: generated.length, sha256: createHash("sha256").update(generated).digest("hex") };
  }
  const preview = await buildReplyPreview(root, temporary, check);
  console.log(JSON.stringify({ check, files: report, preview }));
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
