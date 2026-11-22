import * as path from "node:path";

export const root = path.resolve(__dirname, "..");
export const runtimeMain = path.join(root, "runtime", "main.js");
export const stateDir = path.join(root, "state");
export const testApp = path.join(root, "state", "test-app");
export const backupDir = path.join(root, "state", "backups");
export const installState = path.join(root, "state", "install.json");
export const logs = path.join(root, "logs");
export const userData = path.join(root, "state", "user-data");
export const codexHome = path.join(root, "state", "codex-home");
export const xdgConfig = path.join(root, "state", "xdg-config");
