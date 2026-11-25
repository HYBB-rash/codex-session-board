#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
"$root/node_modules/.bin/tsc" -p "$root/tsconfig.json"
npm run --silent typecheck:ui
"$root/node_modules/.bin/esbuild" src/extensions/session-board/preview.ts \
    --bundle --platform=node --format=cjs --legal-comments=eof --log-level=warning \
    --outfile=dist/extensions/session-board/preview.js
"$root/node_modules/.bin/vite" build --config src/ui/preload.vite.config.ts --logLevel warn
"$root/node_modules/.bin/vite" build --config src/ui/standalone.vite.config.ts --logLevel warn
chmod +x "$root/dist/bin/codex-enhancer.js" "$root/dist/bin/codex-session-board.js"
