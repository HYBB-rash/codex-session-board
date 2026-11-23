import { execFileSync, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { sha256File } from "./asar.js";

const codesign = "/usr/bin/codesign";
const officialRequirement =
    'anchor apple generic and identifier "com.openai.codex" and certificate leaf[subject.OU] = "2DC432GLL2"';

export function verifyMacSignature(bundle: string, official = false): string {
    const requirement = official ? [`-R=${officialRequirement}`] : [];
    execFileSync(codesign, ["--verify", "--deep", "--strict", ...requirement, bundle]);
    const result = spawnSync(codesign, ["--display", "--verbose=4", bundle], {
        encoding: "utf8",
        stdio: ["ignore", "ignore", "pipe"],
    });
    if (result.error) throw result.error;
    const hash = result.status === 0 && /^CDHash=([a-f0-9]{40})$/m.exec(result.stderr)?.[1];
    if (!hash) throw new Error("missing macOS code directory hash");
    return hash;
}

export function verifyOfficialCopy(bundle: string, archive: string): string {
    const codeHash = verifyMacSignature(bundle, true);
    const resources = path.join(bundle, "Contents/_CodeSignature/CodeResources");
    const relative = path.relative(path.join(bundle, "Contents"), archive);
    const seal = execFileSync(
        "/usr/bin/plutil",
        [
            "-extract",
            `files2.${relative.replaceAll(".", "\\.")}.hash2`,
            "raw",
            "-o",
            "-",
            resources,
        ],
        { encoding: "utf8" },
    );
    if (Buffer.from(seal.trim(), "base64").toString("hex") !== sha256File(archive)) {
        throw new Error("ASAR bytes do not match OpenAI's resource seal");
    }
    return codeHash;
}

function sign(target: string, entitlements?: string): void {
    const permissions = entitlements ? ["--entitlements", entitlements] : [];
    execFileSync(codesign, ["--force", "--sign", "-", ...permissions, target], {
        stdio: "inherit",
    });
}

function signHelpers(directory: string, entitlementsFile: string): void {
    for (const helper of fs.readdirSync(directory, { withFileTypes: true })) {
        if (!helper.isDirectory() || !helper.name.endsWith(".app")) continue;
        const bundle = path.join(directory, helper.name);
        const entitlements = execFileSync(
            codesign,
            ["-d", "--entitlements", "-", "--xml", bundle],
            {
                stdio: ["ignore", "pipe", "pipe"],
            },
        );
        if (entitlements.length > 0) {
            fs.writeFileSync(entitlementsFile, entitlements, { mode: 0o600 });
        }
        sign(bundle, entitlements.length > 0 ? entitlementsFile : undefined);
    }
}

function signLibraries(directory: string): void {
    for (const library of fs.readdirSync(directory, { withFileTypes: true })) {
        if (library.isFile() && library.name.endsWith(".dylib")) {
            sign(path.join(directory, library.name));
        }
    }
}

export function signEmbeddedCopy(bundle: string, binary: string, workspace: string): string {
    const framework = path.join(bundle, "Contents/Frameworks/Codex Framework.framework");
    const version = path.dirname(binary);
    const entitlements = path.join(workspace, "entitlements.plist");
    signLibraries(path.join(version, "Libraries"));
    signHelpers(path.join(version, "Helpers"), entitlements);
    sign(framework);
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
    sign(bundle, entitlements);
    return verifyMacSignature(bundle);
}
