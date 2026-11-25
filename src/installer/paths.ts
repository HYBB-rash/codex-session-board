import * as path from "node:path";
import { IN_PLACE_ROOT } from "./constants.js";
import { inPlaceUserDirectory } from "./in-place/receipt.js";

export const outputDir = path.resolve(__dirname, "..");
const releaseRoot = path.resolve(outputDir, "..");
const parent = path.dirname(releaseRoot);
const runningInstalledRelease =
    /^[a-f0-9]{64}$/.test(path.basename(releaseRoot)) &&
    path.basename(parent) === "releases" &&
    path.basename(path.dirname(parent)) === "state";
export const root = runningInstalledRelease ? path.resolve(parent, "../..") : releaseRoot;
export const runtimeMain = path.join(outputDir, "runtime", "main.js");
export const inPlaceRuntime = runningInstalledRelease && root === IN_PLACE_ROOT;
export const stateDir = inPlaceRuntime ? inPlaceUserDirectory() : path.join(root, "state");
export const testApp = path.join(
    stateDir,
    process.platform === "darwin" ? "Codex Enhancer.app" : "test-app",
);
export const backupDir = path.join(root, "state", "backups");
export const installState = path.join(root, "state", "install.json");
export const logs = inPlaceRuntime ? path.join(stateDir, "logs") : path.join(root, "logs");
export const userData = path.join(root, "state", "user-data");
export const codexHome = path.join(root, "state", "codex-home");
export const xdgConfig = path.join(root, "state", "xdg-config");
