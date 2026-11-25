import { existsSync, readdirSync, unlinkSync } from "node:fs";
import { join, resolve } from "node:path";
import ts from "typescript";

const root = join(import.meta.dirname, "..");
const ui = join(root, "src", "ui").replaceAll("\\", "/");

function fail(diagnostics) {
    console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, ts.createCompilerHost({})));
    process.exit(1);
}

const config = ts.getParsedCommandLineOfConfigFile(
    join(root, "tsconfig.json"),
    { noEmit: false, noCheck: true, listEmittedFiles: true },
    { ...ts.sys, onUnRecoverableConfigFileDiagnostic: (diagnostic) => fail([diagnostic]) },
);
if (config.errors.length) fail(config.errors);

const nodeFiles = config.fileNames.filter((filename) => !filename.startsWith(`${ui}/`));
const result = ts.createProgram(nodeFiles, config.options).emit();
if (result.diagnostics.length) fail(result.diagnostics);
if (result.emitSkipped) throw new Error("Node JavaScript output was skipped");

const emittedFiles = new Set(result.emittedFiles.map((filename) => resolve(filename)));
for (const group of readdirSync(config.options.rootDir, { withFileTypes: true })) {
    if (!group.isDirectory() || group.name === "ui") continue;
    const output = join(config.options.outDir, group.name);
    if (!existsSync(output)) continue;
    for (const entry of readdirSync(output, { recursive: true, withFileTypes: true })) {
        if (!entry.isFile() || !entry.name.endsWith(".js")) continue;
        const filename = resolve(entry.parentPath, entry.name);
        if (!emittedFiles.has(filename)) unlinkSync(filename);
    }
}
