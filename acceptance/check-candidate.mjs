import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { isAbsolute, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const upstream = "2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc";
const upstreamCommands = [
    "serve",
    "stop",
    "mcp",
    "scan",
    "migrate",
    "migrate-sections",
    "check-sections",
];

function contents(path) {
    assert.ok(
        path && !isAbsolute(path) && normalize(path) === path && !path.startsWith("../"),
        `Invalid evidence path ${path}`,
    );
    return readFileSync(resolve(root, path));
}

function json(path) {
    return JSON.parse(contents(path).toString("utf8"));
}

function sha256(path) {
    return createHash("sha256").update(contents(path)).digest("hex");
}

function keysMatch(entries, expected, label) {
    assert.deepEqual(Object.keys(entries).sort(), [...expected].sort(), label);
}

function verifyReferences(references, evidence, label) {
    assert.ok(Array.isArray(references) && references.length > 0, `${label} lacks evidence`);
    assert.equal(references.length, new Set(references).size, `${label} repeats evidence`);
    for (const reference of references)
        assert.ok(Object.hasOwn(evidence, reference), `${label} has unknown evidence ${reference}`);
}

function verifyEvidence(evidence) {
    for (const [label, reference] of Object.entries(evidence)) {
        const source = contents(reference.file).toString("utf8");
        if (reference.test) {
            assert.ok(
                source.includes(`test(${JSON.stringify(reference.test)}`),
                `${label} names a missing test`,
            );
        } else {
            assert.match(reference.checkpoint_commit, /^[0-9a-f]{7,40}$/, label);
            assert.ok(reference.coverage?.trim(), `${label} lacks real evidence scope`);
        }
    }
}

function verifyEntries(entries, evidence, label, baseline = new Map()) {
    for (const [id, entry] of Object.entries(entries)) {
        const name = `${label}/${id}`;
        assert.ok(["pending", "partial", "fixture"].includes(entry.status), name);
        for (const path of entry.implementation ?? []) contents(path);
        if (entry.status === "fixture") {
            assert.ok(!entry.remaining, `${name} has outstanding assertions`);
            assert.ok(
                !baseline.has(id) || baseline.get(id).verification === "automated",
                `${name} requires a real host`,
            );
        } else {
            assert.ok(entry.remaining?.trim(), `${name} lacks its outstanding assertions`);
        }
        if (entry.status !== "pending" || entry.evidence)
            verifyReferences(entry.evidence, evidence, name);
    }
}

function verifyFrozenContracts(candidate, requirements, registry) {
    const lock = json("acceptance/contract.lock.json");
    assert.equal(lock.schema_version, 1);
    keysMatch(lock.files, ["requirements.json", "model.py"], "frozen acceptance files");
    for (const [file, expected] of Object.entries(lock.files))
        assert.equal(sha256(`acceptance/${file}`), expected, file);
    assert.equal(candidate.schema_version, 1);
    assert.equal(candidate.upstream_commit, upstream);
    assert.equal(
        sha256("src/ui/shared/board/operations.json"),
        candidate.operation_registry_sha256,
    );
    assert.equal(requirements.size, 31, "baseline requirements");
    assert.equal(registry.length, 18, "shared operations");
    keysMatch(candidate.requirements, requirements.keys(), "requirements");
    keysMatch(
        candidate.operations,
        registry.map(({ action }) => action),
        "operations",
    );
    keysMatch(candidate.cli, upstreamCommands, "CLI");
    const cliSource = contents("src/bin/codex-session-board.ts").toString("utf8");
    assert.deepEqual(
        /const commands = "([^"]+)"/.exec(cliSource)?.[1]?.split("|"),
        upstreamCommands,
        "candidate CLI command list",
    );
}

function openAssertions(candidate, registry) {
    const remaining = [];
    for (const group of ["requirements", "cli", "goal_checks"]) {
        for (const [id, entry] of Object.entries(candidate[group]))
            if (entry.remaining) remaining.push(`${id} — ${entry.remaining}`);
    }
    for (const { action, method, path, tool } of registry)
        remaining.push(`${action} (${method} ${path} / ${tool}) — ${candidate.operations[action]}`);
    return remaining;
}

function main() {
    const candidate = json("acceptance/candidate-map.json");
    const baseline = json("acceptance/requirements.json").requirements;
    const requirements = new Map(baseline.map((entry) => [entry.id, entry]));
    const registry = json("src/ui/shared/board/operations.json");
    verifyFrozenContracts(candidate, requirements, registry);
    verifyEvidence(candidate.evidence);
    verifyEntries(candidate.requirements, candidate.evidence, "requirements", requirements);
    verifyEntries(candidate.cli, candidate.evidence, "CLI");
    verifyEntries(candidate.goal_checks, candidate.evidence, "goal");
    verifyReferences(candidate.operation_evidence, candidate.evidence, "operations");
    for (const [action, remaining] of Object.entries(candidate.operations))
        assert.ok(remaining?.trim(), `${action} lacks outstanding assertions`);
    const open = openAssertions(candidate, registry);
    const { length: checked } = Object.keys(candidate.requirements).filter(
        (id) => candidate.requirements[id].status === "fixture",
    );
    console.log(
        `Mapped ${requirements.size} requirements (${checked} fixture-covered), ${registry.length} operations, ` +
            `${upstreamCommands.length} CLI commands and ${Object.keys(candidate.goal_checks).length} goal checks. ` +
            `${open.length} entries have outstanding assertions.`,
    );
    if (process.argv.includes("--open")) console.log(open.join("\n"));
}

main();
