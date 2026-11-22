import assert from "node:assert/strict";
import * as path from "node:path";
import test from "node:test";
import { logs, outputDir, root, runtimeMain, stateDir } from "./paths.js";

test("compiled files sit in dist and runtime data stays in the repository", () => {
  assert.equal(path.basename(outputDir), "dist");
  assert.equal(root, path.dirname(outputDir));
  assert.equal(runtimeMain, path.join(outputDir, "runtime", "main.js"));
  assert.equal(stateDir, path.join(root, "state"));
  assert.equal(logs, path.join(root, "logs"));
});
