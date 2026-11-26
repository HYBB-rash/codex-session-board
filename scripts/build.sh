#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
npm run --silent typecheck
node scripts/emit-node.mjs
"$root/node_modules/.bin/esbuild" src/extensions/preview.ts \
    --bundle --platform=node --format=cjs --legal-comments=eof --log-level=warning \
    --outfile=dist/extensions/preview.js
"$root/node_modules/.bin/esbuild" src/ui/entry/official/inject.ts \
    --bundle --platform=browser --format=iife --target=es2022 --log-level=warning \
    --outfile=dist/official/inject.js
"$root/node_modules/.bin/vite" build --config src/ui/entry/desktop/vite.config.mts --logLevel warn
"$root/node_modules/.bin/vite" build --config src/ui/entry/standalone/vite.config.mts --logLevel warn
chmod +x "$root/dist/bin/codex-enhancer.js" "$root/dist/bin/codex-session-board.js"
