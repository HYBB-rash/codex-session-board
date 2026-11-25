import { lstatSync, readdirSync, readFileSync } from "node:fs";
import * as path from "node:path";
import { sha256File } from "../asar.js";

export const releaseManifestName = "release.json";
export const releasePlatforms = ["linux-x86_64", "macos-arm64"] as const;
export type ReleasePlatform = (typeof releasePlatforms)[number];
export const inPlaceReleaseProtocol = 1;

interface ReleaseFile {
    path: string;
    sha256: string;
}

export interface ReleaseManifest {
    schema: 1;
    version: string;
    platform: ReleasePlatform;
    inPlaceProtocol?: number;
    files: ReleaseFile[];
}

export function releasePlatform(platform = process.platform, arch = process.arch): ReleasePlatform {
    if (platform === "linux" && arch === "x64") return "linux-x86_64";
    if (platform === "darwin" && arch === "arm64") return "macos-arm64";
    throw new Error(`unsupported session board platform ${platform}/${arch}`);
}

export function releaseFiles(directory: string, current = directory): ReleaseFile[] {
    const status = lstatSync(current);
    if (status.isDirectory()) {
        return readdirSync(current)
            .sort()
            .flatMap((name) => releaseFiles(directory, path.join(current, name)));
    }
    if (!status.isFile()) throw new Error(`invalid release file ${current}`);
    const relative = path.relative(directory, current);
    return [{ path: relative.split(path.sep).join("/"), sha256: sha256File(current) }];
}

function filesMatch(expected: ReleaseFile[], actual: ReleaseFile[]): boolean {
    const ordered = [...expected].sort((left, right) => left.path.localeCompare(right.path));
    const found = [...actual].sort((left, right) => left.path.localeCompare(right.path));
    return (
        ordered.length === found.length &&
        ordered.every((file, index) => {
            const value = found[index];
            return value?.path === file.path && value.sha256 === file.sha256;
        })
    );
}

export function requireReleaseVersion(version: unknown): asserts version is string {
    if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:-[\w.-]+)?$/.test(version)) {
        throw new Error("invalid release version");
    }
}

export function versionManifest(
    directory: string,
    version: string,
    platform: ReleasePlatform,
): ReleaseManifest {
    requireReleaseVersion(version);
    return { schema: 1, version, platform, files: releaseFiles(directory) };
}

export function readReleaseManifest(file: string): ReleaseManifest {
    const value: unknown = JSON.parse(readFileSync(file, "utf8"));
    if (typeof value !== "object" || value === null) throw new Error("invalid release manifest");
    const manifest = value as Partial<ReleaseManifest>;
    requireReleaseVersion(manifest.version);
    if (
        manifest.schema !== 1 ||
        typeof manifest.platform !== "string" ||
        !releasePlatforms.some((platform) => platform === manifest.platform) ||
        (manifest.inPlaceProtocol !== undefined &&
            (!Number.isSafeInteger(manifest.inPlaceProtocol) || manifest.inPlaceProtocol < 1)) ||
        !Array.isArray(manifest.files) ||
        !manifest.files.every(
            (entry) =>
                entry &&
                typeof entry.path === "string" &&
                typeof entry.sha256 === "string" &&
                /^[a-f0-9]{64}$/.test(entry.sha256),
        )
    ) {
        throw new Error("invalid release manifest");
    }
    return manifest as ReleaseManifest;
}

export function verifyRelease(packageRoot: string, platform = releasePlatform()): ReleaseManifest {
    const manifest = readReleaseManifest(path.join(packageRoot, releaseManifestName));
    if (manifest.platform !== platform)
        throw new Error(`expected ${platform}, got ${manifest.platform}`);
    const files = releaseFiles(path.join(packageRoot, "dist"));
    if (!filesMatch(manifest.files, files))
        throw new Error("release files differ from their SHA-256 manifest");
    for (const entry of ["bin/codex-enhancer.js", "runtime/main.js", "runtime/preload.js"]) {
        if (!files.some((file) => file.path === entry)) throw new Error(`release lacks ${entry}`);
    }
    return manifest;
}

export function verifyInPlaceRelease(packageRoot: string): ReleaseManifest {
    const manifest = verifyRelease(packageRoot, "linux-x86_64");
    if (manifest.inPlaceProtocol !== inPlaceReleaseProtocol) {
        throw new Error(
            `release ${manifest.version} does not support in-place installation protocol ${inPlaceReleaseProtocol}`,
        );
    }
    return manifest;
}
