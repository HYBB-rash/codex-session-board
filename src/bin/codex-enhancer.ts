#!/usr/bin/env node

import { detect, findSystemApp, formatDetection } from "../installer/detect.js";
import { install } from "../installer/install.js";
import { launch } from "../installer/launch.js";
import { packageRelease } from "../installer/package-release.js";
import { releasePlatform, releasePlatforms } from "../installer/release-manifest.js";
import { activateRelease, activeRelease, rollbackRelease } from "../installer/releases.js";
import { restore } from "../installer/restore.js";
import { formatStatus, status } from "../installer/status.js";

const command = process.argv[2];

function fail(error: unknown): void {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
}

if (command === "detect") {
    try {
        console.log(formatDetection(detect(findSystemApp().appDir)));
    } catch (error) {
        fail(error);
    }
} else if (command === "install") {
    try {
        const result = install();
        console.log(`action: ${result.action}`);
        console.log(`test copy: ${result.state.target.path}`);
        console.log(`codex version: ${result.state.app.codexVersion}`);
        console.log(`runtime: ${result.state.app.runtimeType}`);
        console.log(`entry: ${result.state.app.entry}`);
        console.log(`original sha256: ${result.state.files.archive.originalSha256}`);
        console.log(`patched sha256: ${result.state.files.archive.patchedSha256}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "uninstall") {
    try {
        const result = restore();
        console.log("action: removed");
        console.log(`archive sha256: ${result.archiveHash}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "status") {
    try {
        console.log(formatStatus(status()));
        const release = activeRelease();
        if (release) console.log(`board release: ${release.version}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "launch") {
    try {
        const pid = launch(process.argv.slice(3));
        console.log(`pid: ${pid}`);
    } catch (error) {
        fail(error);
    }
} else if (command === "update" || command === "rollback") {
    try {
        const candidate = process.argv[3];
        if (command === "update" && !candidate)
            throw new Error("provide an extracted board package directory");
        const release =
            candidate && command === "update" ? activateRelease(candidate) : rollbackRelease();
        console.log(`board release: ${release.version}`);
        console.log("Restart the Enhancer copy to load this release.");
    } catch (error) {
        fail(error);
    }
} else if (command === "package") {
    try {
        const [version, directory, target] = process.argv.slice(3);
        if (!version || !directory) throw new Error("provide a version and output directory");
        const platform = target
            ? releasePlatforms.find((value) => value === target)
            : releasePlatform();
        if (!platform) throw new Error(`unsupported package target ${target}`);
        console.log(JSON.stringify(packageRelease(version, directory, platform)));
    } catch (error) {
        fail(error);
    }
} else {
    console.log(
        "Usage: codex-enhancer detect|install|uninstall|status|launch|update <directory>|rollback|package <version> <output> [platform]",
    );
    process.exitCode = command ? 1 : 0;
}
