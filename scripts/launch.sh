#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
npm --prefix "$root" run build --silent
exec node "$root/bin/codex-enhancer.js" launch "$@"
