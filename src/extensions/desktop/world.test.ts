import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveLocalProjectId } from "./world.js";

const roots = ["/workspace/main", "/workspace/shared"];
const projects = {
    "native-project": { rootPaths: roots },
    "different-project": { rootPaths: [roots[0] ?? "", "/workspace/other"] },
};

test("local composer resolves an app-server ID to the native ID by the full project roots", () => {
    assert.equal(resolveLocalProjectId(projects, "server-project", roots), "native-project");
    assert.equal(resolveLocalProjectId(projects, "server-project", ["/workspace/main"]), undefined);
});

test("local composer preserves an exact ID and rejects ambiguous root matches", () => {
    const duplicateRoots = { ...projects, "duplicate-project": { rootPaths: roots } };
    assert.equal(resolveLocalProjectId(duplicateRoots, "native-project", roots), "native-project");
    assert.throws(
        () => resolveLocalProjectId(duplicateRoots, "server-project", roots),
        /多个本机项目使用相同目录/,
    );
});
