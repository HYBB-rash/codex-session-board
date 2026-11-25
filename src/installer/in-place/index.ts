import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { IN_PLACE_APP, IN_PLACE_ROOT, inPlaceLoader } from "../constants.js";
import { verifyInPlaceRelease } from "../release/manifest.js";
import { activateRelease, activeRelease, rollbackRelease } from "../release/store.js";
import {
    archivePosition,
    cleanArchiveStage,
    commitArchive,
    discardArchiveBackup,
    prepareArchive,
    restoreArchive,
    syncDirectory,
    writeInstallationReceipt,
} from "./archive.js";
import { notifyInPlace, readRuntimeFile } from "./bootstrap.js";
import {
    type InPlaceInstallation,
    inPlaceReleaseStore,
    inPlaceTransaction,
    inPlaceUserDirectory,
    newInstallation,
    readInPlaceInstallation,
} from "./receipt.js";
import {
    assertSystemRegistrationClear,
    configureWatcher,
    installControl,
    reloadUserWatchers,
} from "./registration.js";
import {
    assertCompatibleExecutables,
    assertOriginalRpmRecorded,
    assertSystemPath,
    inspectStockArchive,
    readInPlaceBuildVersion,
    requireCompatibleRpm,
} from "./target.js";
import { checkInPlaceUpgrade, type InPlaceWatchStatus } from "./watcher.js";

const rpmLock = "/usr/lib/sysimage/rpm/.rpm.lock";
const receiptStage = /^\.receipt-[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;

function refreshWatchers(enabled: boolean): void {
    for (const warning of reloadUserWatchers(enabled)) console.error(warning);
}

function assertStopped(): void {
    const running = fs.readdirSync("/proc").filter((entry) => {
        if (!/^\d+$/.test(entry)) return false;
        try {
            const program = fs.readlinkSync(`/proc/${entry}/exe`).replace(/ \(deleted\)$/, "");
            if (program.startsWith(`${IN_PLACE_APP}/`)) return true;
            const argv = fs.readFileSync(`/proc/${entry}/cmdline`, "utf8").split("\0");
            return argv.includes(path.join(IN_PLACE_APP, "start.sh"));
        } catch (error) {
            const code = (error as NodeJS.ErrnoException).code;
            if (code === "EACCES" || code === "EPERM") {
                const argv = fs.readFileSync(`/proc/${entry}/cmdline`, "utf8").split("\0");
                return argv.some((word) => word.startsWith(`${IN_PLACE_APP}/`));
            }
            if (code !== "ENOENT" && code !== "ESRCH") throw error;
            return false;
        }
    });
    if (running.length) throw new Error(`请先退出原 Codex 客户端，仍有进程 ${running.join(", ")}`);
}

function withRpmLock<T>(change: () => T, { requiresStopped = true } = {}): T {
    if (process.platform !== "linux" || process.arch !== "x64")
        throw new Error("当前原地安装仅支持已核对的 Fedora x86_64 RPM");
    if (process.getuid?.() !== 0) throw new Error("原地安装修改系统文件，需要管理员权限");
    assertSystemPath(rpmLock, false);
    const fd = fs.openSync(rpmLock, fs.constants.O_RDWR | fs.constants.O_NOFOLLOW);
    const previousMask = process.umask(0o022);
    try {
        try {
            execFileSync("/usr/bin/flock", ["--fcntl", "-x", "-n", "-E", "75", "3"], {
                stdio: ["ignore", "ignore", "pipe", fd],
            });
        } catch (error) {
            if ((error as { status?: number }).status === 75)
                throw new Error("RPM 更新或原地安装事务正在进行，请稍后重试");
            throw error;
        }
        if (requiresStopped) assertStopped();
        return change();
    } finally {
        process.umask(previousMask);
        fs.closeSync(fd);
    }
}

function stockMatches(installation: InPlaceInstallation): boolean {
    const observed = inspectStockArchive();
    return (
        observed.kind === "stock" &&
        observed.package === installation.package &&
        observed.sha256 === installation.original.sha256 &&
        observed.entry === installation.entry
    );
}

function cleanInstallation(installation: InPlaceInstallation): void {
    const transaction = inPlaceTransaction(installation);
    configureWatcher(false);
    refreshWatchers(false);
    cleanArchiveStage(transaction);
    discardArchiveBackup(transaction, installation.original);
    for (const name of ["state", "control", ".control-stage"])
        fs.rmSync(path.join(IN_PLACE_ROOT, name), { recursive: true, force: true });
    for (const name of fs.readdirSync(IN_PLACE_ROOT)) {
        if (receiptStage.test(name)) fs.unlinkSync(path.join(IN_PLACE_ROOT, name));
    }
    fs.unlinkSync(transaction.receipt);
    syncDirectory(IN_PLACE_ROOT);
    if (fs.readdirSync(IN_PLACE_ROOT).length === 0) {
        fs.rmdirSync(IN_PLACE_ROOT);
        syncDirectory(path.dirname(IN_PLACE_ROOT));
    }
}

function resetPreparation(installation: InPlaceInstallation): void {
    if (!stockMatches(installation)) throw new Error("安装准备期间 Codex 已变化，已保留恢复材料");
    const transaction = inPlaceTransaction(installation);
    cleanArchiveStage(transaction);
    const backup = fs.lstatSync(transaction.backup, { throwIfNoEntry: false });
    if (backup) {
        assertSystemPath(transaction.backup, false);
        fs.unlinkSync(transaction.backup);
        syncDirectory(path.dirname(transaction.backup));
    }
}

function unregisteredReceiptStages(): string[] {
    assertSystemRegistrationClear();
    if (!fs.lstatSync(IN_PLACE_ROOT, { throwIfNoEntry: false })) return [];
    assertSystemPath(IN_PLACE_ROOT, true);
    return fs.readdirSync(IN_PLACE_ROOT).map((name) => {
        if (!receiptStage.test(name)) throw new Error("原地安装记录缺失，已保留恢复材料");
        const staging = path.join(IN_PLACE_ROOT, name);
        assertSystemPath(staging, false);
        return staging;
    });
}

function prepareInstaller(): InPlaceInstallation {
    const compatible = requireCompatibleRpm();
    const remnants = unregisteredReceiptStages();
    fs.mkdirSync(IN_PLACE_ROOT, { mode: 0o755, recursive: true });
    fs.chmodSync(IN_PLACE_ROOT, 0o755);
    for (const file of remnants) fs.unlinkSync(file);
    syncDirectory(path.dirname(IN_PLACE_ROOT));
    const installation = newInstallation({
        package: compatible.package,
        version: compatible.build.codexVersion,
        entry: compatible.build.archive.main,
        original: compatible.archive,
    });
    writeInstallationReceipt(inPlaceTransaction(installation).receipt, installation);
    return installation;
}

export function installInPlace(packageRoot: string): {
    action: "installed" | "unchanged";
    version: string;
} {
    return withRpmLock(() => {
        const manifest = verifyInPlaceRelease(packageRoot);
        const prior = readInPlaceInstallation();
        let installation = prior ?? prepareInstaller();
        const transaction = inPlaceTransaction(installation);
        if (installation.phase === "removing") throw new Error("请先完成上一次原地卸载");
        if (installation.phase === "installed") {
            if (archivePosition(transaction, installation) !== "patched")
                throw new Error("Codex 已变化，请先卸载旧登记，再安装兼容版");
            const active = activeRelease(inPlaceReleaseStore());
            if (!active) throw new Error("原地安装没有已激活的看板版本");
            configureWatcher(true);
            refreshWatchers(true);
            return { action: "unchanged", version: active.version };
        }
        try {
            if (installation.phase === "preparing") {
                if (prior) resetPreparation(installation);
                const proofs = prepareArchive(
                    transaction,
                    installation.entry,
                    installation.original,
                    inPlaceLoader(),
                );
                installation = { ...installation, phase: "prepared", ...proofs };
                writeInstallationReceipt(transaction.receipt, installation);
            }
            installControl(packageRoot);
            activateRelease(packageRoot, inPlaceReleaseStore());
            configureWatcher(true);
            commitArchive(transaction, installation, assertStopped);
            writeInstallationReceipt(transaction.receipt, { ...installation, phase: "installed" });
            refreshWatchers(true);
            return { action: "installed", version: manifest.version };
        } catch (error) {
            try {
                if (installation.phase === "preparing") resetPreparation(installation);
                else restoreArchive(transaction, installation, assertStopped);
                cleanInstallation(installation);
            } catch (recovery) {
                throw new AggregateError(
                    [error, recovery],
                    `${String(error)}；恢复未完成：${String(recovery)}`,
                );
            }
            throw error;
        }
    });
}

export function uninstallInPlace(): { action: "restored" | "kept-rpm" | "not-installed" } {
    return withRpmLock(() => {
        const installation = readInPlaceInstallation();
        if (!installation) {
            const remnants = unregisteredReceiptStages();
            if (fs.existsSync(IN_PLACE_ROOT)) {
                for (const file of remnants) fs.unlinkSync(file);
                fs.rmdirSync(IN_PLACE_ROOT);
                syncDirectory(path.dirname(IN_PLACE_ROOT));
            }
            return { action: "not-installed" };
        }
        const transaction = inPlaceTransaction(installation);
        if (installation.phase === "preparing") {
            resetPreparation(installation);
            cleanInstallation(installation);
            return { action: "restored" };
        }
        const position = archivePosition(transaction, installation);
        const observed = position !== "patched" ? inspectStockArchive() : undefined;
        if (position !== "patched" && observed?.kind !== "stock")
            throw new Error("启动归档的归属无法核对，已保留恢复材料");
        if (position === "patched") {
            assertOriginalRpmRecorded(
                transaction.archive,
                installation.package,
                installation.original,
            );
            assertCompatibleExecutables(path.join(IN_PLACE_APP, "ChatGPT"), installation.version);
        }
        writeInstallationReceipt(transaction.receipt, { ...installation, phase: "removing" });
        if (position === "patched") restoreArchive(transaction, installation, assertStopped);
        cleanInstallation(installation);
        return { action: position === "patched" ? "restored" : "kept-rpm" };
    });
}

export function adjustInPlaceRelease(candidate?: string): string {
    return withRpmLock(
        () => {
            const installation = readInPlaceInstallation();
            if (
                installation?.phase !== "installed" ||
                archivePosition(inPlaceTransaction(installation), installation) !== "patched"
            )
                throw new Error("当前 Codex 的原地安装记录无法核对");
            const store = inPlaceReleaseStore();
            const release = candidate ? activateRelease(candidate, store) : rollbackRelease(store);
            return release.version;
        },
        { requiresStopped: false },
    );
}

function patchedStatus(installation: InPlaceInstallation): InPlaceWatchStatus {
    const version = readInPlaceBuildVersion();
    try {
        if (version !== installation.version)
            throw new Error(`构建与已登记的 ${installation.version} 不同`);
        assertCompatibleExecutables(path.join(IN_PLACE_APP, "ChatGPT"), version);
        readRuntimeFile();
        return { kind: "ready" };
    } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        return {
            kind: "incompatible",
            fingerprint: createHash("sha256").update(`${version}\n${detail}`).digest("hex"),
            message: `Codex ${version} 的会话看板已停用。${detail}。请安装兼容版看板或卸载原地安装。`,
        };
    }
}

export function inPlaceStatus(): InPlaceWatchStatus {
    try {
        const installation = readInPlaceInstallation();
        if (!installation) {
            if (unregisteredReceiptStages().length)
                throw new Error("首次原地安装在写入记录时中断，请重试安装或卸载");
            return { kind: "not-installed" };
        }
        const archive = inPlaceTransaction(installation);
        if (installation.phase !== "installed")
            throw new Error(`原地安装事务尚未完成，阶段 ${installation.phase}`);
        if (archivePosition(archive, installation) === "patched")
            return patchedStatus(installation);
        const observed = inspectStockArchive();
        if (observed.kind === "stock") {
            return {
                kind: "loader-replaced",
                fingerprint: `${observed.package}/${observed.entry}/${observed.sha256}`,
                message: `Codex ${observed.version} 的 RPM 已替换会话看板入口。请安装兼容版看板或清理旧登记。`,
            };
        }
        const detail = observed.kind === "unknown" ? observed.detail : "Codex RPM 文件不存在";
        throw new Error(detail);
    } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        return {
            kind: "unverifiable",
            fingerprint: createHash("sha256").update(detail).digest("hex"),
            message: `无法核对 Codex 会话看板原地安装。${detail}。请保留恢复材料并检查安装状态。`,
        };
    }
}

export function purgeInPlaceData(): boolean {
    if (process.getuid?.() === 0) throw new Error("请用运行 Codex 的普通用户清理看板数据");
    if (inPlaceStatus().kind !== "not-installed") throw new Error("请先完成会话看板的原地卸载");
    const directory = inPlaceUserDirectory();
    const info = fs.lstatSync(directory, { throwIfNoEntry: false });
    if (!info) return false;
    if (!info.isDirectory() || info.uid !== process.getuid?.())
        throw new Error("当前用户的看板数据路径无法核对");
    fs.rmSync(directory, { recursive: true });
    return true;
}

function statusAfterRpm(): InPlaceWatchStatus {
    assertSystemPath(rpmLock, false);
    const fd = fs.openSync(rpmLock, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    try {
        execFileSync("/usr/bin/flock", ["--fcntl", "-s", "-w", "25", "3"], {
            stdio: ["ignore", "ignore", "pipe", fd],
        });
        return inPlaceStatus();
    } finally {
        fs.closeSync(fd);
    }
}

export function runInPlaceCheck() {
    return checkInPlaceUpgrade(inPlaceUserDirectory(), statusAfterRpm, ({ message }) =>
        notifyInPlace(message),
    );
}
