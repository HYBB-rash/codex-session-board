import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { entryIncludes, sha256File } from "./asar.js";
import { IN_PLACE_APP, IN_PLACE_ARCHIVE, SENTINEL } from "./constants.js";
import { type Detection, detect, findStartupArchive } from "./detect.js";

const compatibleBuild = {
    version: "26.1111.71704",
    runtime: "owl",
    entry: ".vite/build/early-bootstrap.js",
    archiveSha256: "20c53634196da5d0a84b7cc5c8113a7370aa4ec0b78fffeb6cda3c03f138301e",
    binarySha256: "48db1e43e1692275688084bd9b13ba68fbd2dde9710ca139a574f39ef6f0c80d",
} as const;

export interface FileProof {
    sha256: string;
    mode: number;
    uid: number;
    gid: number;
    mtimeNs: string;
    attributes: string;
}

interface RpmFile {
    package: string;
    sha256: string;
    mode: number;
}

export type InstalledArchive =
    | { kind: "missing" }
    | { kind: "unknown"; detail: string }
    | {
          kind: "stock";
          package: string;
          version: string;
          archive: string;
          entry: string;
          sha256: string;
      };

export function assertSystemPath(item: string, directory: boolean): fs.Stats {
    let inspected = item;
    let expectedDirectory = directory;
    for (;;) {
        const info = fs.lstatSync(inspected);
        if (
            (expectedDirectory ? !info.isDirectory() : !info.isFile()) ||
            info.uid !== 0 ||
            (info.mode & 0o022) !== 0 ||
            (!expectedDirectory && info.nlink !== 1)
        ) {
            throw new Error(`系统路径的所有者、权限或文件类型异常：${inspected}`);
        }
        const parent = path.dirname(inspected);
        if (parent === inspected) break;
        inspected = parent;
        expectedDirectory = true;
    }
    return fs.lstatSync(item);
}

function rpmFile(file: string): RpmFile {
    const query =
        "%{NEVRA}\\t%{FILEDIGESTALGO}\\n[%{FILENAMES}\\t%{FILEDIGESTS}\\t%{FILEMODES}\\t%{FILEUSERNAME}\\t%{FILEGROUPNAME}\\n]";
    const [header, ...rows] = execFileSync("/usr/bin/rpm", ["-qf", "--qf", query, file], {
        encoding: "utf8",
        env: { PATH: "/usr/bin:/bin", LC_ALL: "C" },
        stdio: ["ignore", "pipe", "pipe"],
    })
        .trim()
        .split("\n");
    const [name, digestAlgorithm] = header?.split("\t") ?? [];
    const [, digest, mode, user, group] =
        rows.map((row) => row.split("\t")).find(([filename]) => filename === file) ?? [];
    if (
        !name?.startsWith("codex-desktop-") ||
        !name.endsWith(".x86_64") ||
        digestAlgorithm !== "8" ||
        !digest ||
        !/^[a-f0-9]{64}$/.test(digest) ||
        user !== "root" ||
        group !== "root" ||
        !mode ||
        !/^\d+$/.test(mode)
    ) {
        throw new Error(`RPM 没有可核对的系统文件记录：${file}`);
    }
    return { package: name, sha256: digest, mode: Number(mode) & 0o7777 };
}

export function fileProof(file: string): FileProof {
    const info = fs.lstatSync(file, { bigint: true });
    if (!info.isFile() || info.nlink !== 1n) throw new Error(`预期为独立的常规文件：${file}`);
    const attributes = execFileSync(
        "/usr/bin/getfattr",
        ["-h", "-d", "-m", "-", "-e", "hex", "--", file],
        {
            encoding: "utf8",
            stdio: ["ignore", "pipe", "pipe"],
            env: { PATH: "/usr/bin:/bin", LC_ALL: "C" },
        },
    )
        .split("\n")
        .filter((line) => line.includes("=") && !line.startsWith("#"))
        .sort()
        .join("\n");
    return {
        sha256: sha256File(file),
        mode: Number(info.mode & 0o7777n),
        uid: Number(info.uid),
        gid: Number(info.gid),
        mtimeNs: info.mtimeNs.toString(),
        attributes,
    };
}

export function sameFileProof(left: FileProof, right: FileProof): boolean {
    return (
        left.sha256 === right.sha256 &&
        left.mode === right.mode &&
        left.uid === right.uid &&
        left.gid === right.gid &&
        left.mtimeNs === right.mtimeNs &&
        left.attributes === right.attributes
    );
}

function assertRpmFile(file: string, rpm: RpmFile, sha256: string): void {
    const info = assertSystemPath(file, false);
    if (sha256 !== rpm.sha256 || info.gid !== 0 || (info.mode & 0o7777) !== rpm.mode)
        throw new Error(`文件与 RPM 记录不同：${file}`);
}

export function assertCompatibleExecutable(binary: string): void {
    assertSystemPath(binary, false);
    if (sha256File(binary) !== compatibleBuild.binarySha256)
        throw new Error("Codex 可执行文件与经过验收的构建不同");
}

export function assertOriginalRpmRecorded(
    file: string,
    recordedPackage: string,
    original: FileProof,
): void {
    const rpm = rpmFile(file);
    if (
        rpm.package !== recordedPackage ||
        rpm.sha256 !== original.sha256 ||
        rpm.mode !== original.mode
    )
        throw new Error("RPM 已换包但旧补丁仍在，已保留恢复材料");
}

export function inspectStockArchive(app = IN_PLACE_APP): InstalledArchive {
    if (!fs.existsSync(app)) return { kind: "missing" };
    try {
        assertSystemPath(path.join(app, "resources"), true);
        const archive = findStartupArchive(app);
        assertSystemPath(archive.archivePath, false);
        const rpm = rpmFile(archive.archivePath);
        const sha256 = sha256File(archive.archivePath);
        assertRpmFile(archive.archivePath, rpm, sha256);
        if (entryIncludes(archive.archivePath, archive.main, Buffer.from(SENTINEL)))
            throw new Error("RPM 启动入口仍包含看板 Loader");
        const build = detect(app);
        return {
            kind: "stock",
            package: rpm.package,
            version: build.codexVersion,
            archive: archive.relativeArchive,
            entry: archive.main,
            sha256,
        };
    } catch (error) {
        return { kind: "unknown", detail: error instanceof Error ? error.message : String(error) };
    }
}

export function requireCompatibleRpm(app = IN_PLACE_APP): {
    build: Detection;
    package: string;
    archive: FileProof;
} {
    if (process.platform !== "linux" || process.arch !== "x64")
        throw new Error("原地安装目前仅支持已核对的 Fedora x86_64 RPM");
    assertSystemPath(path.join(app, "resources"), true);
    assertSystemPath(path.join(app, IN_PLACE_ARCHIVE), false);
    assertSystemPath(path.join(app, "ChatGPT"), false);
    const build = detect(app);
    if (
        build.codexVersion !== compatibleBuild.version ||
        build.runtimeType !== compatibleBuild.runtime ||
        build.archive.main !== compatibleBuild.entry ||
        build.archive.relativeArchive !== IN_PLACE_ARCHIVE
    ) {
        throw new Error(`Codex ${build.codexVersion} 尚未通过原地安装兼容性验证`);
    }
    const archive = fileProof(build.archive.archivePath);
    const owner = rpmFile(build.archive.archivePath);
    const binaryOwner = rpmFile(build.binary);
    if (
        archive.sha256 !== compatibleBuild.archiveSha256 ||
        binaryOwner.sha256 !== compatibleBuild.binarySha256 ||
        owner.package !== binaryOwner.package
    ) {
        throw new Error("Codex RPM 的构建指纹尚未通过原地安装兼容性验证");
    }
    assertRpmFile(build.archive.archivePath, owner, archive.sha256);
    assertRpmFile(build.binary, binaryOwner, sha256File(build.binary));
    return { build, package: owner.package, archive };
}
