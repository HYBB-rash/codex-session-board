import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import * as path from "node:path";
import { outputDir, stateDir } from "../paths.js";

export interface LauncherInstall {
    path: string;
}

function quote(value: string): string {
    return `'${value.replaceAll("'", "'\\''")}'`;
}

function infoPlist(): string {
    return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Codex 看板</string>
    <key>CFBundleDisplayName</key><string>Codex 看板</string>
    <key>CFBundleIdentifier</key><string>com.hongyong.codex-board-launcher</string>
    <key>CFBundleExecutable</key><string>launcher</string>
    <key>CFBundleIconFile</key><string>app.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
`;
}

function launcherScript(startArgs: readonly string[]): string {
    const cli = path.join(outputDir, "bin", "codex-enhancer.js");
    const argv = ["official", "start", ...startArgs].map(quote).join(" ");
    const receipt = path.join(stateDir, "official", "launch.json");
    const homebrew = "/opt/homebrew/bin/node";
    const node = existsSync(homebrew) ? homebrew : process.execPath;
    return `#!/bin/zsh
set -u
node=${quote(node)}
log="$HOME/Library/Logs/codex-board-launcher.log"
mkdir -p "$HOME/Library/Logs"
if ! "$node" ${quote(cli)} ${argv} >>"$log" 2>&1; then
    /usr/bin/osascript -e 'display notification "Codex 已在运行但没有看板入口。请先完全退出 Codex，再从这个图标启动。" with title "Codex 看板"' >/dev/null 2>&1 || true
    exit 1
fi
app="$("$node" -e 'const fs=require("fs");try{process.stdout.write(JSON.parse(fs.readFileSync(process.argv[1],"utf8")).appDir)}catch{}' ${quote(receipt)} 2>/dev/null)"
if [ -n "$app" ]; then
    /usr/bin/open -a "$app"
fi
`;
}

export function installLauncher(
    options: { destination?: string; startArgs?: readonly string[]; iconSource?: string } = {},
): LauncherInstall {
    const destination =
        options.destination ?? path.join(homedir(), "Applications", "Codex 看板.app");
    const contents = path.join(destination, "Contents");
    const macos = path.join(contents, "MacOS");
    const resources = path.join(contents, "Resources");
    mkdirSync(macos, { recursive: true, mode: 0o755 });
    mkdirSync(resources, { recursive: true, mode: 0o755 });
    const icon = options.iconSource ?? "/Applications/ChatGPT.app/Contents/Resources/app.icns";
    if (existsSync(icon)) copyFileSync(icon, path.join(resources, "app.icns"));
    const script = path.join(macos, "launcher");
    writeFileSync(script, launcherScript(options.startArgs ?? []), { mode: 0o755 });
    chmodSync(script, 0o755);
    writeFileSync(path.join(contents, "Info.plist"), infoPlist());
    writeFileSync(path.join(contents, "PkgInfo"), "APPL????");
    execFileSync("/usr/bin/codesign", ["--force", "--deep", "--sign", "-", destination], {
        stdio: "pipe",
    });
    return { path: destination };
}
