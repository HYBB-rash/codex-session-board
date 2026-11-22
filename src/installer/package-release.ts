import { execFileSync } from "node:child_process";
import {
    cpSync,
    existsSync,
    lstatSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    rmSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { sha256File } from "./asar.js";
import { outputDir, root } from "./paths.js";
import {
    type ReleasePlatform,
    releaseManifestName,
    releasePlatform,
    requireReleaseVersion,
    versionManifest,
} from "./release-manifest.js";

const acceptanceFiles = [
    "README.md",
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    "biome.json",
    "acceptance",
    "src",
    "ui",
    "scripts",
    "docs/session-board-native-checkpoint.md",
    "docs/session-board-typescript-acceptance.md",
];

function packageAcceptance(directory: string): void {
    for (const file of acceptanceFiles) {
        cpSync(path.join(root, file), path.join(directory, file), {
            recursive: true,
            filter: (source) => {
                const name = path.basename(source);
                return (
                    !["node_modules", "__pycache__"].includes(name) &&
                    !name.startsWith(".") &&
                    !lstatSync(source).isSymbolicLink()
                );
            },
        });
    }
}

function packagePlugin(directory: string, version: string): void {
    const manifest = ".codex-plugin/plugin.json";
    const source = path.join(root, manifest);
    if (!existsSync(source)) return;
    const destination = path.join(directory, manifest);
    const metadata = JSON.parse(readFileSync(source, "utf8")) as Record<string, unknown>;
    mkdirSync(path.dirname(destination));
    writeFileSync(destination, `${JSON.stringify({ ...metadata, version }, null, 4)}\n`);
    cpSync(path.join(root, ".mcp.json"), path.join(directory, ".mcp.json"));
}

export function packageRelease(
    version: string,
    output: string,
    platform: ReleasePlatform = releasePlatform(),
): { archive: string; sha256: string } {
    requireReleaseVersion(version);
    const name = `session-board-${version}-${platform}`;
    const work = mkdtempSync(path.join(tmpdir(), "session-board-package-"));
    const directory = path.join(work, name);
    try {
        mkdirSync(directory);
        cpSync(outputDir, path.join(directory, "dist"), { recursive: true });
        const versionFile = path.join(directory, "dist", "board-assets", "version.txt");
        if (existsSync(versionFile)) writeFileSync(versionFile, `${version}\n`);
        packagePlugin(directory, version);
        packageAcceptance(directory);
        const installGuide = path.join(root, "scripts", "package-install.md");
        if (existsSync(installGuide)) cpSync(installGuide, path.join(directory, "INSTALL.md"));
        const manifest = versionManifest(path.join(directory, "dist"), version, platform);
        writeFileSync(
            path.join(directory, releaseManifestName),
            `${JSON.stringify(manifest, null, 2)}\n`,
        );
        mkdirSync(output, { recursive: true });
        const archive = path.resolve(output, `${name}.tar.gz`);
        execFileSync("tar", ["-czf", archive, "-C", work, name], { stdio: "inherit" });
        const sha256 = sha256File(archive);
        writeFileSync(`${archive}.sha256`, `${sha256}  ${path.basename(archive)}\n`);
        return { archive, sha256 };
    } finally {
        rmSync(work, { recursive: true, force: true });
    }
}
