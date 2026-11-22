import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { entryIncludes, sha256ArchiveHeader, sha256File } from "./asar.js";
import { IN_PLACE_APP, IN_PLACE_ARCHIVE, LINK_NAME, SENTINEL } from "./constants.js";
import { type Detection, findSystemApp } from "./detect.js";
import { inPlaceTransaction, readInPlaceInstallation } from "./in-place-receipt.js";
import { assertSystemPath } from "./in-place-target.js";
import * as paths from "./paths.js";

export function copyFile(source: string, destination: string): void {
    fs.copyFileSync(source, destination, fs.constants.COPYFILE_FICLONE);
}

function inside(root: string, item: string): boolean {
    return item === root || item.startsWith(`${root}${path.sep}`);
}

function resolvedExisting(item: string): string {
    const absolute = path.resolve(item);
    if (fs.lstatSync(absolute, { throwIfNoEntry: false })) return fs.realpathSync(absolute);
    const parent = path.dirname(absolute);
    if (absolute === parent) return absolute;
    return path.join(resolvedExisting(parent), path.basename(absolute));
}

export function assertIsolatedPath(item: string, source?: string): void {
    const location = resolvedExisting(item);
    const copy = resolvedExisting(paths.testApp);
    const expected = path.join(
        resolvedExisting(path.dirname(paths.testApp)),
        path.basename(paths.testApp),
    );
    if (!inside(copy, location) || copy !== expected) {
        throw new Error(`refusing to modify outside the Enhancer copy at ${paths.testApp}`);
    }
    const system = resolvedExisting(source ?? findSystemApp().appDir);
    if (inside(system, location) || inside(copy, system)) {
        throw new Error(`refusing to modify the installed Codex at ${system}`);
    }
}

function useStockArchive(source: Detection, candidate: string): void {
    const archive = path.join(candidate, source.archive.relativeArchive);
    if (!entryIncludes(archive, source.archive.main, Buffer.from(SENTINEL))) return;
    if (
        source.platform !== "linux" ||
        source.appDir !== IN_PLACE_APP ||
        source.archive.relativeArchive !== IN_PLACE_ARCHIVE
    )
        throw new Error("首次创建副本需要原始归档或已登记的 Fedora 原地安装");
    const receipt = readInPlaceInstallation();
    if (
        receipt?.phase !== "installed" ||
        receipt.entry !== source.archive.main ||
        receipt.version !== source.codexVersion ||
        sha256File(archive) !== receipt.patched.sha256
    )
        throw new Error("原地补丁与安装记录不符，无法初始化副本");
    const { backup } = inPlaceTransaction(receipt);
    assertSystemPath(backup, false);
    const original = `${archive}.stock`;
    fs.copyFileSync(backup, original, fs.constants.COPYFILE_EXCL | fs.constants.COPYFILE_FICLONE);
    if (sha256File(original) !== receipt.original.sha256)
        throw new Error("原地安装备份与原件指纹不符，无法初始化副本");
    fs.renameSync(original, archive);
}

export function ensureTestCopy(source: Detection): void {
    assertIsolatedPath(paths.testApp, source.appDir);
    if (fs.existsSync(paths.testApp)) {
        assertIsolatedPath(path.join(paths.testApp, source.archive.relativeArchive), source.appDir);
        return;
    }
    fs.mkdirSync(paths.stateDir, { recursive: true, mode: 0o700 });
    const staging = fs.mkdtempSync(path.join(paths.stateDir, ".app-copy-"));
    const candidate = path.join(staging, path.basename(paths.testApp));
    try {
        if (source.platform === "darwin") {
            execFileSync("/usr/bin/ditto", [source.appDir, candidate], { stdio: "inherit" });
        } else {
            execFileSync("cp", ["-a", "--reflink=auto", source.appDir, candidate], {
                stdio: "inherit",
            });
        }
        useStockArchive(source, candidate);
        fs.renameSync(candidate, paths.testApp);
    } finally {
        fs.rmSync(staging, { recursive: true, force: true });
    }
}

function linuxCopyPids(): string[] {
    return fs.readdirSync("/proc").filter((entry) => {
        if (!/^\d+$/.test(entry) || entry === String(process.pid)) return false;
        try {
            const args = fs.readFileSync(`/proc/${entry}/cmdline`, "utf8").split("\0");
            return args.some((argument) => argument.startsWith(`${paths.testApp}/`));
        } catch {
            return false;
        }
    });
}

function macCopyPids(): string[] {
    const processes = execFileSync("/bin/ps", ["-ax", "-o", "pid=,comm="], { encoding: "utf8" });
    return processes.split("\n").flatMap((line) => {
        const match = /^\s*(\d+)\s+(.*)$/.exec(line);
        return match?.[1] && match[2]?.startsWith(`${paths.testApp}/`) ? [match[1]] : [];
    });
}

export function assertTestCopyStopped(): void {
    const pids = process.platform === "darwin" ? macCopyPids() : linuxCopyPids();
    if (pids.length > 0)
        throw new Error(`quit the Enhancer copy before changing it (pids ${pids.join(", ")})`);
}

export function linkFile(resourcesDirectory: string): string {
    return path.resolve(resourcesDirectory, process.platform === "darwin" ? "." : "..", LINK_NAME);
}

function bundlePlist(copy: string): string {
    return path.join(copy, "Contents", "Info.plist");
}

function signBundle(copy: string): void {
    const entitlements = path.join(paths.backupDir, "adhoc-entitlements.plist");
    fs.mkdirSync(paths.backupDir, { recursive: true, mode: 0o700 });
    const keys = [
        "com.apple.security.cs.allow-jit",
        "com.apple.security.cs.allow-unsigned-executable-memory",
    ];
    const values = keys.map((key) => `<key>${key}</key><true/>`).join("");
    fs.writeFileSync(
        entitlements,
        `<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>${values}</dict></plist>`,
        { mode: 0o600 },
    );
    execFileSync(
        "/usr/bin/codesign",
        ["--force", "--sign", "-", "--entitlements", entitlements, copy],
        { stdio: "inherit" },
    );
    execFileSync("/usr/bin/codesign", ["--verify", "--deep", "--strict", copy], {
        stdio: "inherit",
    });
}

function updateIntegrity(plist: string, relativeArchive: string, digest: string): void {
    const archive = relativeArchive.replace(/^Contents\//, "");
    const key = archive.replaceAll(".", "\\.");
    const value = JSON.stringify({ algorithm: "SHA256", hash: digest });
    const entry = `ElectronAsarIntegrity.${key}`;
    if (!hasIntegrityManifest(plist)) {
        execFileSync(
            "/usr/bin/plutil",
            [
                "-insert",
                "ElectronAsarIntegrity",
                "-json",
                `{${JSON.stringify(archive)}:${value}}`,
                plist,
            ],
            { stdio: "inherit" },
        );
        return;
    }
    const output = execFileSync(
        "/usr/bin/plutil",
        ["-extract", "ElectronAsarIntegrity", "json", "-o", "-", plist],
        { encoding: "utf8" },
    );
    const integrity = JSON.parse(output) as Record<string, object>;
    const action = Object.hasOwn(integrity, archive) ? "-replace" : "-insert";
    execFileSync("/usr/bin/plutil", [action, entry, "-json", value, plist], { stdio: "inherit" });
}

function hasIntegrityManifest(plist: string): boolean {
    const value: unknown = JSON.parse(
        execFileSync("/usr/bin/plutil", ["-convert", "json", "-o", "-", plist], {
            encoding: "utf8",
        }),
    );
    return value !== null && typeof value === "object" && "ElectronAsarIntegrity" in value;
}

export function backupMacMetadata(copy: Detection, backupArchive: string): void {
    if (copy.platform !== "darwin") return;
    const plist = bundlePlist(copy.appDir);
    assertIsolatedPath(plist);
    const backup = `${backupArchive}.Info.plist`;
    if (!fs.existsSync(backup)) copyFile(plist, backup);
}

export function signPatchedCopy(copy: Detection): void {
    if (copy.platform !== "darwin") return;
    const plist = bundlePlist(copy.appDir);
    assertIsolatedPath(plist);
    const fuse = "EnableEmbeddedAsarIntegrityValidation";
    if (copy.fuses?.flags[fuse] || hasIntegrityManifest(plist)) {
        const digest = sha256ArchiveHeader(copy.archive.archivePath);
        updateIntegrity(plist, copy.archive.relativeArchive, digest);
    }
    signBundle(copy.appDir);
}

export function restoreMacMetadata(copy: string, backupArchive: string): void {
    if (process.platform !== "darwin") return;
    const plist = bundlePlist(copy);
    assertIsolatedPath(plist);
    copyFile(`${backupArchive}.Info.plist`, plist);
    signBundle(copy);
}
