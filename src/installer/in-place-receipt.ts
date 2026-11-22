import { randomUUID } from "node:crypto";
import * as fs from "node:fs";
import { homedir } from "node:os";
import * as path from "node:path";
import { IN_PLACE_APP, IN_PLACE_ARCHIVE, IN_PLACE_RECEIPT, IN_PLACE_ROOT } from "./constants.js";
import type { ArchiveTransaction } from "./in-place-archive.js";
import { assertSystemPath, type FileProof } from "./in-place-target.js";

interface OriginalInstallation {
    schema: 1;
    transactionId: string;
    package: string;
    version: string;
    entry: string;
    original: FileProof;
}

export type InPlaceInstallation = OriginalInstallation &
    ({ phase: "preparing" } | { phase: "prepared" | "installed" | "removing"; patched: FileProof });

export function newInstallation(
    details: Omit<OriginalInstallation, "schema" | "transactionId">,
): InPlaceInstallation {
    return { schema: 1, transactionId: randomUUID(), ...details, phase: "preparing" };
}

function isMetadata(value: unknown): value is FileProof {
    if (!value || typeof value !== "object") return false;
    const proof = value as Partial<FileProof>;
    return (
        typeof proof.sha256 === "string" &&
        /^[a-f0-9]{64}$/.test(proof.sha256) &&
        Number.isInteger(proof.mode) &&
        proof.uid === 0 &&
        proof.gid === 0 &&
        typeof proof.mtimeNs === "string" &&
        /^\d+$/.test(proof.mtimeNs) &&
        typeof proof.attributes === "string"
    );
}

export function readInPlaceInstallation(): InPlaceInstallation | null {
    const receipt = path.join(IN_PLACE_ROOT, IN_PLACE_RECEIPT);
    try {
        assertSystemPath(receipt, false);
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
        throw error;
    }
    const value: unknown = JSON.parse(fs.readFileSync(receipt, "utf8"));
    const installation = value as Partial<
        OriginalInstallation & { phase: unknown; patched: FileProof }
    > | null;
    if (
        installation?.schema !== 1 ||
        typeof installation.transactionId !== "string" ||
        !/^[a-f0-9-]{36}$/.test(installation.transactionId) ||
        typeof installation.package !== "string" ||
        !installation.package.startsWith("codex-desktop-") ||
        typeof installation.version !== "string" ||
        !/^[\w.-]+$/.test(installation.version) ||
        typeof installation.entry !== "string" ||
        installation.entry.startsWith("/") ||
        installation.entry.split("/").includes("..") ||
        !isMetadata(installation.original) ||
        !(
            installation.phase === "preparing" ||
            (["prepared", "installed", "removing"].includes(String(installation.phase)) &&
                isMetadata(installation.patched))
        )
    )
        throw new Error("原地安装记录无法核对");
    return installation as InPlaceInstallation;
}

export function inPlaceTransaction(installation: InPlaceInstallation): ArchiveTransaction {
    const archive = path.join(IN_PLACE_APP, IN_PLACE_ARCHIVE);
    return {
        archive,
        staged: path.join(
            path.dirname(archive),
            `.codex-enhancer-${installation.transactionId}.stage`,
        ),
        backup: path.join(IN_PLACE_ROOT, "original.asar"),
        receipt: path.join(IN_PLACE_ROOT, IN_PLACE_RECEIPT),
    };
}

export function inPlaceUserDirectory(): string {
    const { XDG_STATE_HOME: xdg } = process.env;
    return path.join(
        xdg && path.isAbsolute(xdg) ? xdg : path.join(homedir(), ".local/state"),
        "codex-enhancer/in-place",
    );
}

export function inPlaceReleaseStore(): string {
    return path.join(IN_PLACE_ROOT, "state/releases");
}
