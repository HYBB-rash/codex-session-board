#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
"$root/node_modules/.bin/tsc" -p "$root/tsconfig.json"
mkdir -p "$root/bin" "$root/installer" "$root/runtime" "$root/extensions/hello-world"
cp "$root/dist/bin/codex-enhancer.js" "$root/bin/codex-enhancer.js"
cp "$root/dist/installer/"*.js "$root/installer/"
cp "$root/dist/runtime/"*.js "$root/runtime/"
cp "$root/dist/extensions/hello-world/extension.js" "$root/extensions/hello-world/extension.js"
chmod +x "$root/bin/codex-enhancer.js"
rm -rf "$root/dist"
