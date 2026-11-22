#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
"$root/node_modules/.bin/tsc" -p "$root/tsconfig.json"
"$root/node_modules/.bin/vite" build --config ui/preload.vite.config.ts --logLevel warn
"$root/node_modules/.bin/vite" build --config ui/standalone.vite.config.ts --logLevel warn
chmod +x "$root/dist/bin/codex-enhancer.js" "$root/dist/bin/codex-session-board.js"
