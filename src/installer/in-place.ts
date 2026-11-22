import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { IN_PLACE_APP, IN_PLACE_ROOT, inPlaceLoader } from "./constants.js";
import {
    archivePosition,
    cleanArchiveStage,
    commitArchive,
    discardArchiveBackup,
    prepareArchive,
    restoreArchive,
    syncDirectory,
    writeInstallationReceipt,
} from "./in-place-archive.js";
import { notifyInPlace } from "./in-place-bootstrap.js";
import {
    type InPlaceInstallation,
    inPlaceReleaseStore,
    inPlaceTransaction,
    inPlaceUserDirectory,
    newInstallation,
    readInPlaceInstallation,
} from "./in-place-receipt.js";
import {
    assertSystemRegistrationClear,
    configureWatcher,
    installControl,
} from "./in-place-registration.js";
import { assertSystemPath, inspectStockArchive, requireCompatibleRpm } from "./in-place-target.js";
import { checkInPlaceUpgrade, type InPlaceWatchStatus } from "./in-place-watcher.js";
import { verifyRelease } from "./release-manifest.js";
import { activateRelease, rollbackRelease } from "./releases.js";

const rpmLock = "/usr/lib/sysimage/rpm/.rpm.lock";

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

function withRpmLock<T>(change: () => T): T {
    if (process.platform !== "linux" || process.arch !== "x64")
        throw new Error("当前原地安装仅支持已核对的 Fedora x86_64 RPM");
    if (process.getuid?.() !== 0) throw new Error("原地安装修改系统文件，需要管理员权限");
    assertSystemPath(rpmLock, false);
    const fd = fs.openSync(rpmLock, fs.constants.O_RDWR | fs.constants.O_NOFOLLOW);
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
        assertStopped();
        return change();
    } finally {
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
    cleanArchiveStage(transaction);
    discardArchiveBackup(transaction, installation.original);
    for (const name of ["state", "control", ".control-stage"])
        fs.rmSync(path.join(IN_PLACE_ROOT, name), { recursive: true, force: true });
    for (const name of fs.readdirSync(IN_PLACE_ROOT)) {
        if (/^\.receipt-[a-f0-9-]{36}$/.test(name)) fs.unlinkSync(path.join(IN_PLACE_ROOT, name));
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

function prepareInstaller(): InPlaceInstallation {
    const compatible = requireCompatibleRpm();
    assertSystemRegistrationClear();
    fs.mkdirSync(IN_PLACE_ROOT, { mode: 0o755, recursive: true });
    assertSystemPath(IN_PLACE_ROOT, true);
    if (fs.readdirSync(IN_PLACE_ROOT).length !== 0)
        throw new Error("原地安装目录有未知文件，已保留恢复材料");
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
        const manifest = verifyRelease(packageRoot);
        const prior = readInPlaceInstallation();
        let installation = prior ?? prepareInstaller();
        const transaction = inPlaceTransaction(installation);
        if (installation.phase === "removing") throw new Error("请先完成上一次原地卸载");
        if (installation.phase === "installed") {
            if (archivePosition(transaction, installation) !== "patched")
                throw new Error("Codex 已变化，请先卸载旧登记，再安装兼容版");
            return { action: "unchanged", version: installation.version };
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
        if (!installation) return { action: "not-installed" };
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
        writeInstallationReceipt(transaction.receipt, { ...installation, phase: "removing" });
        if (position === "patched") restoreArchive(transaction, installation, assertStopped);
        cleanInstallation(installation);
        return { action: position === "patched" ? "restored" : "kept-rpm" };
    });
}

export function adjustInPlaceRelease(candidate?: string): string {
    return withRpmLock(() => {
        const installation = readInPlaceInstallation();
        if (
            installation?.phase !== "installed" ||
            archivePosition(inPlaceTransaction(installation), installation) !== "patched"
        )
            throw new Error("当前 Codex 的原地安装记录无法核对");
        const store = inPlaceReleaseStore();
        const release = candidate ? activateRelease(candidate, store) : rollbackRelease(store);
        return release.version;
    });
}

export function inPlaceStatus(): InPlaceWatchStatus {
    try {
        const installation = readInPlaceInstallation();
        if (!installation) return { kind: "not-installed" };
        const archive = inPlaceTransaction(installation);
        if (installation.phase !== "installed")
            throw new Error(`原地安装事务尚未完成，阶段 ${installation.phase}`);
        if (archivePosition(archive, installation) === "patched") {
            const metadata = JSON.parse(
                fs.readFileSync(
                    path.join(IN_PLACE_APP, "resources/linux-package-metadata.json"),
                    "utf8",
                ),
            ) as { version?: unknown };
            if (metadata.version === installation.version) return { kind: "ready" };
            return {
                kind: "incompatible",
                fingerprint: createHash("sha256").update(String(metadata.version)).digest("hex"),
                message: `Codex ${String(metadata.version)} 与已登记的 ${installation.version} 不同。请安装兼容版看板或卸载。`,
            };
        }
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

export function runInPlaceCheck() {
    return checkInPlaceUpgrade(inPlaceUserDirectory(), inPlaceStatus, ({ message }) =>
        notifyInPlace(message),
    );
}
