import { randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, readdirSync, renameSync, rmdirSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { databaseName, existingDatabase, snapshotDatabase } from "./database.js";
import {
    assertAbsent,
    canonicalLeaf,
    canonicalSource,
    digest,
    directory,
    inspect,
    readRegular,
    regularFile,
    writePrivate,
} from "./files.js";

const hostsDirectory = "remote-hosts";
const sourceSidecars = ["source.json", "scan-progress.json"] as const;
const configName = "config.json";
const receiptName = "migration.json";
const snapshotName = "snapshot";
const hostHashPattern = /^[0-9a-f]{64}$/;

export interface LegacyMigrationOptions {
    sourceConfig: string;
    destination: string;
    defaultSourceData?: string;
}

interface Source {
    data: string;
    config: Buffer;
}

function defaultDataPath(): string {
    const { XDG_DATA_HOME: xdg } = process.env;
    return join(
        xdg && isAbsolute(xdg) ? xdg : join(homedir(), ".local", "share"),
        "codex-session-board",
    );
}

interface FileReceipt {
    name: string;
    sha256: string;
}

export interface LegacyMigrationReceipt {
    version: 1;
    id: string;
    consistency: "per_database";
    mainDatabase: boolean;
    remoteHosts: string[];
    files: FileReceipt[];
}

export interface LegacyMigration {
    directory: string;
    receipt: LegacyMigrationReceipt;
}

function loadSource(options: LegacyMigrationOptions): Source {
    const configPath = canonicalSource(options.sourceConfig);
    const config = readRegular(configPath);
    const value: unknown = JSON.parse(config.toString("utf8"));
    if (typeof value !== "object" || !value || Array.isArray(value))
        throw new Error("旧配置必须为 JSON 对象");
    const data =
        (value as { data_dir?: unknown }).data_dir ??
        options.defaultSourceData ??
        defaultDataPath();
    if (typeof data !== "string" || !data.trim()) throw new Error("旧配置缺少有效的 data_dir");
    const source = canonicalSource(resolve(dirname(configPath), data));
    directory(source);
    return { data: source, config };
}

async function copyDatabase(
    source: string,
    destination: string,
    relative: string,
    files: FileReceipt[],
) {
    const from = join(source, relative);
    const to = join(destination, relative);
    if (!existingDatabase(from)) return false;
    await snapshotDatabase(from, to);
    files.push({ name: relative, sha256: digest(to) });
    return true;
}

function copySidecar(from: string, to: string, relative: string, files: FileReceipt[]) {
    const source = join(from, relative);
    if (!regularFile(source)) return;
    const target = join(to, relative);
    writePrivate(target, readRegular(source));
    files.push({ name: relative, sha256: digest(target) });
}

async function copyHosts(
    source: string,
    destination: string,
    files: FileReceipt[],
): Promise<string[]> {
    const root = join(source, hostsDirectory);
    if (!directory(root)) return [];
    const hosts = readdirSync(root, { withFileTypes: true }).sort((left, right) =>
        left.name.localeCompare(right.name),
    );
    const names: string[] = [];
    mkdirSync(join(destination, hostsDirectory), { mode: 0o700 });
    for (const host of hosts) {
        const hostPath = join(root, host.name);
        if (!inspect(hostPath)?.isDirectory()) continue;
        if (!hostHashPattern.test(host.name))
            throw new Error(`旧主机目录不符合主机身份哈希格式：${host.name}`);
        const relative = join(hostsDirectory, host.name);
        mkdirSync(join(destination, relative), { mode: 0o700 });
        await copyDatabase(source, destination, join(relative, databaseName), files);
        for (const sidecar of sourceSidecars) {
            copySidecar(source, destination, join(relative, sidecar), files);
        }
        names.push(host.name);
    }
    return names;
}

async function prepare(source: Source, stage: string): Promise<LegacyMigrationReceipt> {
    const files: FileReceipt[] = [];
    const config = join(stage, configName);
    writePrivate(config, source.config);
    files.push({ name: configName, sha256: digest(config) });
    const mainDatabase = await copyDatabase(source.data, stage, databaseName, files);
    const remoteHosts = await copyHosts(source.data, stage, files);
    const receipt: LegacyMigrationReceipt = {
        version: 1,
        id: randomUUID(),
        consistency: "per_database",
        mainDatabase,
        remoteHosts,
        files,
    };
    writePrivate(join(stage, receiptName), `${JSON.stringify(receipt, null, 4)}\n`);
    return receipt;
}

export async function migrateLegacyBoard(
    options: LegacyMigrationOptions,
): Promise<LegacyMigration> {
    const destination = canonicalLeaf(options.destination);
    assertAbsent(destination);
    const source = loadSource(options);
    const targetWithinSource = relative(source.data, destination);
    if (
        !targetWithinSource ||
        (targetWithinSource !== ".." && !targetWithinSource.startsWith(`..${sep}`))
    ) {
        throw new Error("迁移目标不能位于旧数据目录中");
    }
    const stage = mkdtempSync(join(dirname(destination), ".board-migration-"));
    try {
        const receipt = await prepare(source, stage);
        mkdirSync(destination, { mode: 0o700 });
        const published = join(destination, snapshotName);
        try {
            renameSync(stage, published);
        } catch (error) {
            rmdirSync(destination);
            throw error;
        }
        return { directory: published, receipt };
    } finally {
        rmSync(stage, { recursive: true, force: true });
    }
}

function allowedReceiptFile(name: unknown): name is string {
    if (name === configName || name === databaseName) return true;
    if (typeof name !== "string") return false;
    const [folder, host, file, rest] = name.split("/");
    return (
        folder === hostsDirectory &&
        host !== undefined &&
        hostHashPattern.test(host) &&
        (file === databaseName || sourceSidecars.some((sidecar) => sidecar === file)) &&
        rest === undefined
    );
}

function validReceipt(value: unknown): value is LegacyMigrationReceipt {
    if (!value || typeof value !== "object") return false;
    const entry = value as Partial<LegacyMigrationReceipt>;
    if (
        !(
            entry.version === 1 &&
            typeof entry.id === "string" &&
            /^[0-9a-f-]{36}$/.test(entry.id) &&
            entry.consistency === "per_database" &&
            typeof entry.mainDatabase === "boolean" &&
            Array.isArray(entry.remoteHosts) &&
            entry.remoteHosts.every(
                (name) => typeof name === "string" && hostHashPattern.test(name),
            ) &&
            Array.isArray(entry.files) &&
            entry.files.every(
                (file) =>
                    file &&
                    allowedReceiptFile(file.name) &&
                    typeof file.sha256 === "string" &&
                    /^[0-9a-f]{64}$/.test(file.sha256),
            )
        )
    )
        return false;
    const hosts = new Set(entry.remoteHosts);
    const names = new Set(entry.files.map(({ name }) => name));
    return (
        hosts.size === entry.remoteHosts.length &&
        names.size === entry.files.length &&
        names.has(configName) &&
        names.has(databaseName) === entry.mainDatabase &&
        [...names].every(
            (name) => !name.startsWith(`${hostsDirectory}/`) || hosts.has(name.split("/")[1] ?? ""),
        )
    );
}

export function readLegacyMigration(destination: string): LegacyMigration {
    const root = canonicalLeaf(destination);
    const snapshot = join(root, snapshotName);
    if (!directory(root) || !directory(snapshot)) throw new Error("迁移尚未发布");
    const receipt: unknown = JSON.parse(readRegular(join(snapshot, receiptName)).toString("utf8"));
    if (!validReceipt(receipt)) throw new Error("迁移凭证无效");
    return { directory: snapshot, receipt };
}

export function verifyLegacyMigration(migration: LegacyMigration): void {
    for (const host of migration.receipt.remoteHosts) {
        const parent = join(migration.directory, hostsDirectory);
        if (!directory(parent) || !directory(join(parent, host)))
            throw new Error(`缺少迁移主机目录：${host}`);
    }
    for (const file of migration.receipt.files) {
        if (digest(join(migration.directory, file.name)) !== file.sha256) {
            throw new Error(`迁移文件内容已改变：${file.name}`);
        }
    }
}

function assertManagedDirectory(migration: LegacyMigration): void {
    const root = dirname(migration.directory);
    if (readdirSync(root).some((entry) => entry !== snapshotName))
        throw new Error("迁移目录含有额外文件");
    const allowed = new Set(migration.receipt.files.map(({ name }) => name));
    allowed.add(receiptName);
    const check = (relativeName: string): void => {
        const absolute = join(migration.directory, relativeName);
        for (const child of readdirSync(absolute)) {
            const name = join(relativeName, child);
            if (allowed.has(name)) continue;
            const host =
                relativeName === hostsDirectory && migration.receipt.remoteHosts.includes(child);
            if ((name === hostsDirectory || host) && directory(join(migration.directory, name)))
                check(name);
            else throw new Error(`迁移目录含有额外文件：${name}`);
        }
    };
    check("");
}

export function rollbackLegacyMigration(destination: string, id: string): string {
    const migration = readLegacyMigration(destination);
    if (migration.receipt.id !== id) throw new Error("迁移编号不匹配");
    verifyLegacyMigration(migration);
    assertManagedDirectory(migration);
    const source = dirname(migration.directory);
    const retained = mkdtempSync(join(dirname(source), ".board-migration-rollback-"));
    try {
        const backup = join(retained, "migration");
        renameSync(source, backup);
        return backup;
    } catch (error) {
        rmdirSync(retained);
        throw error;
    }
}
