#!/usr/bin/bash
set -euo pipefail

if [[ ! -f /run/.containerenv || $EUID != 0 || $(ls /sys/class/net) != lo ]]; then
    printf '%s\n' '请在使用 --network=none 的一次性 Fedora Podman 容器里运行。' >&2
    exit 2
fi
if [[ $# != 4 ]]; then
    printf '%s\n' '用法：fedora-rpm-smoke.sh <Codex RPM> <Codex 构建> <看板归档> <看板版本>' >&2
    exit 2
fi

rpm_file=$(realpath "$1")
build=$2
archive=$(realpath "$3")
version=$4
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[[:alnum:]_.-]+)?$ ]]
root=$(mktemp -d)
cd "$(dirname "$archive")"
sha256sum -c -- "$(basename "$archive").sha256"
rpm -Uvh --replacepkgs "$rpm_file"
tar -xzf "$archive" -C "$root"
package="$root/session-board-$version-linux-x86_64"
node "$package/dist/bin/codex-enhancer.js" install --in-place |
    jq -e --arg version "$version" '.action == "installed" and .version == $version'

script=$(realpath "$(dirname "${BASH_SOURCE[0]}")/fedora-rpm-fixture.cjs")
bootstrap=/var/lib/codex-enhancer/control/dist/installer/in-place-bootstrap.js
printf '\n' >> "$bootstrap"
cat "$script" >> "$bootstrap"

profile=$(mktemp -d)
runtime_log="$profile/.local/state/codex-enhancer/in-place/logs/runtime.jsonl"
install -d -o tester -g tester -m 700 "$profile/run"
chown tester:tester "$profile"
started=$(date +%s)
runuser -u tester -- env -i HOME="$profile" PATH=/usr/bin:/bin \
    XDG_RUNTIME_DIR="$profile/run" DISPLAY=:78 XDG_SESSION_TYPE=x11 \
    CODEX_ENHANCER_SMOKE_HOME="$profile" \
    CODEX_LINUX_DISABLE_USAGE_REPORTING=1 /usr/bin/bash -c '
        set -e
        Xvfb "$DISPLAY" -nolisten tcp -screen 0 1280x800x24 > "$HOME/xvfb.log" 2>&1 &
        trap '\''kill "$!" 2>/dev/null || :'\'' EXIT
        for attempt in {1..50}; do
            [[ -S /tmp/.X11-unix/X78 ]] && break
            sleep .1
        done
        dbus-run-session -- /usr/bin/codex-desktop --ozone-platform=x11
    ' > "$profile/window.log" 2>&1 &

until [[ -s "$runtime_log" ]] && jq -e --arg build "$build" --arg version "$version" '
    select(.event == "main-loaded" and .codexVersion == $build and .boardVersion == $version)
' "$runtime_log" >/dev/null; do
    if (( $(date +%s) - started >= 30 )); then
        printf '%s\n' "未启动指定的 Codex $build 和看板 $version。" >&2
        exit 1
    fi
    sleep .4
done

summary=
while (( $(date +%s) - started < 65 )); do
    if result=$(curl -fsS --max-time 32 http://127.0.0.1:18768/api/sessions 2>/dev/null |
        jq -ce '{
            localSessions: ([.sessions[] | select(.hostId == "local")] | length),
            health: (.health | {connected, scanComplete, lastSuccessAt, error})
        }'); then
        summary=$result
        if jq -e --argjson started "$started" '
            .health | .connected == true and .scanComplete == true
                and (.lastSuccessAt | type) == "number" and .lastSuccessAt >= $started
                and .error == null
        ' <<< "$summary" >/dev/null; then
            jq -c --arg build "$build" --arg version "$version" \
                '{codexBuild: $build, boardVersion: $version} + .' <<< "$summary"
            exit 0
        fi
        if jq -e '.health.error != null' <<< "$summary" >/dev/null; then break; fi
    fi
    sleep .4
done
printf '看板本机会话检查失败：%s\n' "${summary:-看板服务没有返回会话快照}" >&2
exit 1
