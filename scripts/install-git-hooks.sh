#!/usr/bin/bash
set -euo pipefail
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"
hook_dir="$(git rev-parse --git-path hooks)"
case "$hook_dir" in
    /*) ;;
    *) hook_dir="$root/$hook_dir" ;;
esac
mkdir -p "$hook_dir"
ln -sfn "$root/scripts/hooks/pre-commit" "$hook_dir/pre-commit"
chmod +x "$root/scripts/hooks/pre-commit"
