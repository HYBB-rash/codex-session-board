#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
npm run --silent typecheck
node scripts/emit-node.mjs
"$root/node_modules/.bin/esbuild" src/extensions/session-board/preview.ts \
    --bundle --platform=node --format=cjs --legal-comments=eof --log-level=warning \
    --outfile=dist/extensions/session-board/preview.js
"$root/node_modules/.bin/vite" build --config src/ui/desktop/vite.config.mts --logLevel warn
"$root/node_modules/.bin/vite" build --config src/ui/standalone/vite.config.mts --logLevel warn
chmod +x "$root/dist/bin/codex-enhancer.js" "$root/dist/bin/codex-session-board.js"
