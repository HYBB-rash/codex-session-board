import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { IN_PLACE_APP, IN_PLACE_ARCHIVE, IN_PLACE_ROOT } from "./constants.js";
import { presentFile, syncDirectory } from "./in-place-archive.js";
import { assertSystemPath } from "./in-place-target.js";
import { systemWatcherFiles, systemWatcherLinks } from "./in-place-watcher.js";
import { releaseManifestName, verifyInPlaceRelease } from "./release-manifest.js";

const control = path.join(IN_PLACE_ROOT, "control");
const stagedControl = path.join(IN_PLACE_ROOT, ".control-stage");
const node = "/usr/bin/node";
const watcher = systemWatcherFiles({
    node,
    cli: path.join(control, "dist/bin/codex-enhancer.js"),
    watch: path.dirname(path.join(IN_PLACE_APP, IN_PLACE_ARCHIVE)),
});

export function assertSystemRegistrationClear(): void {
    for (const file of [...watcher.keys(), ...systemWatcherLinks().keys()]) {
        if (fs.lstatSync(file, { throwIfNoEntry: false }))
            throw new Error(`已有同名的升级检查器登记：${file}`);
    }
}

export function installControl(packageRoot: string): void {
    if (fs.existsSync(control)) {
        assertSystemPath(control, true);
        verifyInPlaceRelease(control);
        return;
    }
    fs.rmSync(stagedControl, { recursive: true, force: true });
    fs.mkdirSync(stagedControl, { mode: 0o755 });
    fs.cpSync(path.join(packageRoot, "dist"), path.join(stagedControl, "dist"), {
        recursive: true,
    });
    fs.copyFileSync(
        path.join(packageRoot, releaseManifestName),
        path.join(stagedControl, releaseManifestName),
    );
    verifyInPlaceRelease(stagedControl);
    for (const item of fs.readdirSync(stagedControl, { recursive: true, withFileTypes: true })) {
        const file = path.join(item.parentPath, item.name);
        fs.chownSync(file, 0, 0);
        if (item.isDirectory()) {
            fs.chmodSync(file, 0o755);
        } else {
            fs.chmodSync(file, file.endsWith("/codex-session-board.js") ? 0o755 : 0o644);
            const fd = fs.openSync(file, "r");
            try {
                fs.fsyncSync(fd);
            } finally {
                fs.closeSync(fd);
            }
        }
    }
    for (const item of fs
        .readdirSync(stagedControl, { recursive: true, withFileTypes: true })
        .reverse()) {
        if (item.isDirectory()) syncDirectory(path.join(item.parentPath, item.name));
    }
    syncDirectory(stagedControl);
    fs.renameSync(stagedControl, control);
    syncDirectory(IN_PLACE_ROOT);
}

function hasOwnedUnit(file: string, contents: string, link: boolean): boolean {
    const info = fs.lstatSync(file, { throwIfNoEntry: false });
    if (!info) return false;
    const matches =
        info.uid === 0 &&
        (link
            ? info.isSymbolicLink() && fs.readlinkSync(file) === contents
            : info.isFile() && info.nlink === 1 && fs.readFileSync(file, "utf8") === contents);
    if (!matches) throw new Error(`升级检查登记已变化：${file}`);
    return true;
}

export function configureWatcher(enabled: boolean): void {
    for (const [file, contents] of [...watcher, ...systemWatcherLinks()]) {
        const link = !watcher.has(file);
        if (hasOwnedUnit(file, contents, link)) {
            if (!enabled) fs.unlinkSync(file);
        } else if (enabled) {
            const directory = path.dirname(file);
            fs.mkdirSync(directory, { recursive: true, mode: 0o755 });
            assertSystemPath(directory, true);
            if (link) {
                fs.symlinkSync(contents, file);
            } else {
                const next = `${file}.codex-enhancer-stage`;
                if (presentFile(next)) fs.unlinkSync(next);
                fs.writeFileSync(next, contents, { flag: "wx", mode: 0o644, flush: true });
                fs.renameSync(next, file);
            }
        } else if (!link && presentFile(`${file}.codex-enhancer-stage`)) {
            fs.unlinkSync(`${file}.codex-enhancer-stage`);
        }
        if (fs.existsSync(path.dirname(file))) syncDirectory(path.dirname(file));
    }
}

const systemctlOptions = {
    encoding: "utf8" as const,
    env: { PATH: "/usr/bin:/bin", LC_ALL: "C" },
    timeout: 7000,
};

function userManager(entry: { uid?: unknown; user?: unknown }): {
    user: string;
    call: (...args: string[]) => void;
} | null {
    const { uid, user } = entry ?? {};
    if (
        typeof uid !== "number" ||
        !Number.isSafeInteger(uid) ||
        uid < 1 ||
        typeof user !== "string" ||
        !/^[\w.-]+$/.test(user)
    )
        return null;
    const runtime = `/run/user/${uid}`;
    const bus = path.join(runtime, "bus");
    const info = fs.lstatSync(bus, { throwIfNoEntry: false });
    if (!info?.isSocket() || info.uid !== uid) return null;
    const environment = [`XDG_RUNTIME_DIR=${runtime}`, `DBUS_SESSION_BUS_ADDRESS=unix:path=${bus}`];
    return {
        user,
        call: (...args) => {
            execFileSync(
                "/usr/bin/runuser",
                [
                    "-u",
                    user,
                    "--",
                    "/usr/bin/env",
                    ...environment,
                    "/usr/bin/systemctl",
                    "--user",
                    ...args,
                ],
                { ...systemctlOptions, stdio: ["ignore", "pipe", "pipe"] },
            );
        },
    };
}

export function reloadUserWatchers(enabled: boolean): string[] {
    const units = [...watcher.keys()].map((file) => path.basename(file));
    const pathUnit = units.find((unit) => unit.endsWith(".path"));
    if (!pathUnit) throw new Error("缺少路径检查单元");
    try {
        const result: unknown = JSON.parse(
            execFileSync("/usr/bin/loginctl", ["list-users", "--json=short"], {
                ...systemctlOptions,
                stdio: ["ignore", "pipe", "pipe"],
            }),
        );
        if (!Array.isArray(result)) throw new Error("loginctl 未返回用户列表");
        return result.flatMap((entry: { uid?: unknown; user?: unknown }) => {
            const manager = userManager(entry);
            if (!manager) return [];
            try {
                if (!enabled) manager.call("stop", ...units);
                manager.call("daemon-reload");
                if (enabled) manager.call("start", pathUnit);
                return [];
            } catch {
                return [
                    `用户 ${manager.user} 的升级检查器未即时${enabled ? "启动" : "停止"}，下次登录将读取当前登记`,
                ];
            }
        });
    } catch {
        return ["未能刷新已登录用户的升级检查器，下次登录将读取当前登记"];
    }
}
