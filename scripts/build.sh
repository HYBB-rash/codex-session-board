#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
"$root/node_modules/.bin/tsc" -p "$root/tsconfig.json"
node --input-type=module << 'EOF'
import fs from "node:fs";
function strip(source) {
  return source
    .replace(/^"use strict";\n/m, "")
    .replace(/^\/\/\/ <reference lib="dom" \/>\n/m, "")
    .replace(/^Object\.defineProperty\(exports, "__esModule", \{ value: true \}\);\n/m, "")
    .replace(/^exports\.\w+ = .*;\n/gm, "")
    .replace(/^const \w+ = require\("\.\/[^"]+"\);\n/gm, "");
}
const registry = strip(fs.readFileSync("dist/runtime/registry.js", "utf8"));
const host = strip(fs.readFileSync("dist/runtime/host.js", "utf8")).replaceAll("registry_js_1.parseUiPayload", "parseUiPayload");
const preload = fs.readFileSync("dist/runtime/preload.js", "utf8")
  .replace('const host_js_1 = require("./host.js");\n', "")
  .replaceAll("(0, host_js_1.startUiHost)", "startUiHost");
if (preload.includes("./host.js") || host.includes("registry_js_1") || host.includes("./registry.js")) {
  throw new Error("preload bundle still imports a sibling module");
}
fs.writeFileSync("dist/runtime/preload.js", `"use strict";\n${registry}\n${host}\n${preload}`);
EOF
mkdir -p "$root/bin" "$root/installer" "$root/runtime"
cp "$root/dist/bin/codex-enhancer.js" "$root/bin/codex-enhancer.js"
cp "$root/dist/installer/"*.js "$root/installer/"
cp "$root/dist/runtime/"*.js "$root/runtime/"
for extension in "$root/dist/extensions"/*; do
  name="$(basename "$extension")"
  mkdir -p "$root/extensions/$name"
  cp "$extension/extension.js" "$root/extensions/$name/extension.js"
done
chmod +x "$root/bin/codex-enhancer.js"
rm -rf "$root/dist"
