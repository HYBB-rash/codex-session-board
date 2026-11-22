import * as fs from "node:fs";
import * as path from "node:path";
import { entryIncludes, sha256File } from "./asar.js";
import { SENTINEL } from "./constants.js";
import { type Detection, detect, findSystemApp } from "./detect.js";
import { type InstallState, readState } from "./install.js";

export interface Assessment {
    result: "not-installed" | "patch-stale" | "removed" | "installed";
    reasons: string[];
    hash?: string;
    sentinel?: boolean;
}

export interface StatusReport {
    state: InstallState | null;
    system: Detection;
    copy: Detection | null;
    assessment: Assessment;
    upstreamChanged: boolean;
}

export function assess(state: InstallState | null, current: Detection): Assessment {
    const reasons: string[] = [];
    if (!state) return { result: "not-installed", reasons };
    const archivePath = path.join(state.target.path, state.files.archive.path);
    if (!fs.existsSync(archivePath)) {
        return { result: "patch-stale", reasons: ["archive-missing"] };
    }
    const hash = sha256File(archivePath);
    const sentinel = entryIncludes(archivePath, state.app.entry, Buffer.from(SENTINEL));
    if (state.status === "removed") {
        if (hash !== state.files.archive.originalSha256)
            reasons.push("archive-hash-differs-from-original");
        if (sentinel) reasons.push("loader-still-present");
        return {
            result: reasons.length === 0 ? "removed" : "patch-stale",
            reasons,
            hash,
            sentinel,
        };
    }
    if (hash !== state.files.archive.patchedSha256) reasons.push("archive-hash-changed");
    if (!sentinel) reasons.push("loader-missing");
    if (current.codexVersion !== state.app.codexVersion) reasons.push("copy-version-changed");
    if (current.runtimeType !== state.app.runtimeType) reasons.push("runtime-type-changed");
    if (current.archive.main !== state.app.entry) reasons.push("startup-entry-changed");
    return {
        result: reasons.length === 0 ? "installed" : "patch-stale",
        reasons,
        hash,
        sentinel,
    };
}

export function status(): StatusReport {
    const state = readState();
    const system = detect(findSystemApp().appDir);
    let copy: Detection | null = null;
    let assessment: Assessment = { result: "not-installed", reasons: [] };
    if (state && fs.existsSync(state.target.path)) {
        copy = detect(state.target.path);
        assessment = assess(state, copy);
    } else if (state) {
        assessment = { result: "patch-stale", reasons: ["test-copy-missing"] };
    }
    const upstreamChanged = Boolean(state && system.codexVersion !== state.source.codexVersion);
    return { state, system, copy, assessment, upstreamChanged };
}

export function formatStatus(report: StatusReport): string {
    const lines = [
        `result: ${report.assessment.result}`,
        `system version: ${report.system.codexVersion}`,
        `system runtime: ${report.system.runtimeType}`,
    ];
    if (report.upstreamChanged && report.state) {
        lines.push(
            `upstream changed: ${report.state.source.codexVersion} -> ${report.system.codexVersion}`,
        );
    }
    if (report.assessment.reasons.length > 0)
        lines.push(`reasons: ${report.assessment.reasons.join(", ")}`);
    if (report.copy) lines.push(`test copy: ${report.copy.appDir}`);
    return lines.join("\n");
}
