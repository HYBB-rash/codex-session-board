#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if [[ "$(uname -sm)" != "Linux x86_64" ]]; then
    printf '%s\n' 'install:copy 适用于 Linux x86_64。' >&2
    exit 1
fi

destination="${XDG_DATA_HOME:-$HOME/.local/share}/codex-enhancer"
mkdir -p -- "$destination"
destination="$(cd -- "$destination" && pwd -P)"
deployment="$destination/preview"
exec 9>"$destination/preview.lock"
flock 9

staging="$(mktemp -d -- "$destination/.preview.XXXXXXXX")"
trap 'rm -rf -- "$staging"' EXIT
version="$(node -p 'require("./package.json").version')-local.$(date -u +%Y%m%d%H%M%S%N)"
package="session-board-$version-linux-x86_64"
bash scripts/package.sh "$version" "$staging" linux-x86_64
cd -- "$staging"
sha256sum -c -- "$package.tar.gz.sha256"
tar -xzf "$package.tar.gz"

candidate="$staging/$package"
cli="$deployment/dist/bin/codex-enhancer.js"
if [[ ! -e "$deployment" ]]; then
    mv -T -- "$candidate" "$deployment"
    candidate="$deployment"
fi

node -e 'require(process.argv[1]).assertTestCopyStopped(process.argv[2])' \
    "$candidate/dist/installer/copy.js" "$deployment/state/test-app"
copy_status="$(node -p 'require(process.argv[1]).readState()?.status' \
    "$deployment/dist/installer/install.js")"
if [[ "$copy_status" != installed ]]; then
    node "$cli" install
fi
if [[ "$candidate" != "$deployment" ]]; then
    node "$cli" update "$candidate"
fi

auth_home="$(realpath -m -- "${CODEX_HOME:-$HOME/.codex}")"
auth_source="$auth_home/auth.json"
preview_home="$deployment/state/codex-home"
preview_auth="$preview_home/auth.json"
if [[ -f "$auth_source" && "$auth_source" != "$preview_auth" ]]; then
    if [[ ! -e "$preview_auth" && ! -L "$preview_auth" ]]; then
        mkdir -p -m 700 -- "$preview_home"
        ln -sT -- "$auth_source" "$preview_auth"
    elif [[ ! "$preview_auth" -ef "$auth_source" ]]; then
        printf '%s\n' '副本已有独立登录缓存，已保留该缓存。' >&2
    fi
fi

printf '副本目录：%s\n' "$deployment"
CODEX_ENHANCER_LOG_STDOUT=1 node "$cli" launch
