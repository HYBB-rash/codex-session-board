#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

system_node=/usr/bin/node
version="$("$system_node" -p 'require("./package.json").version')-local.$(date -u +%Y%m%d%H%M%S%N)"
candidate_dir="$(mktemp -d)"
trap 'rm -rf -- "$candidate_dir"' EXIT
package="session-board-$version-linux-x86_64"

bash scripts/package.sh "$version" "$candidate_dir" linux-x86_64
cd -- "$candidate_dir"
sha256sum -c -- "$package.tar.gz.sha256"
tar -xzf "$package.tar.gz"

candidate="$candidate_dir/$package"
cli="$candidate/dist/bin/codex-enhancer.js"
status="$("$system_node" "$cli" status --in-place)"
kind="$("$system_node" -p 'JSON.parse(process.argv[1]).kind' "$status")"
case "$kind" in
    ready)
        sudo "$system_node" "$cli" update --in-place "$candidate"
        ;;
    not-installed)
        result="$(sudo "$system_node" "$cli" install --in-place)"
        printf '%s\n' "$result"
        if [[ "$("$system_node" -p 'JSON.parse(process.argv[1]).version' "$result")" != "$version" ]]; then
            printf '%s\n' '原地安装状态已变化，请重新执行 npm run install:in-place。' >&2
            exit 1
        fi
        ;;
    *)
        printf '%s\n' "$status" >&2
        exit 1
        ;;
esac
printf '%s\n' '请从原来的 Codex 图标启动，或完全退出后重新启动，以加载新版本。'
