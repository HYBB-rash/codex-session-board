import { join } from "node:path";
import ts from "typescript";

const root = join(import.meta.dirname, "..");
const ui = join(root, "src", "ui").replaceAll("\\", "/");

function fail(diagnostics) {
    console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, ts.createCompilerHost({})));
    process.exit(1);
}

const config = ts.getParsedCommandLineOfConfigFile(
    join(root, "tsconfig.json"),
    { noEmit: false, noCheck: true },
    { ...ts.sys, onUnRecoverableConfigFileDiagnostic: (diagnostic) => fail([diagnostic]) },
);
if (config.errors.length) fail(config.errors);

const nodeFiles = config.fileNames.filter((filename) => !filename.startsWith(`${ui}/`));
const result = ts.createProgram(nodeFiles, config.options).emit();
if (result.diagnostics.length) fail(result.diagnostics);
if (result.emitSkipped) throw new Error("Node JavaScript output was skipped");
