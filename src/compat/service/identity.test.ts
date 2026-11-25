import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { appendFileSync, cpSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { type TestContext, test } from "node:test";
import { promisify } from "node:util";
import { sha256File } from "../../installer/asar.js";
import {
    releaseManifestName,
    releasePlatform,
    versionManifest,
} from "../../installer/release/manifest.js";
import { activateRelease } from "../../installer/release/store.js";
import { assetsDirectory } from "../operations.js";
import { startBoardServer } from "../server.js";
import { readReceipt, writeReceipt } from "./receipt.js";

const output = dirname(assetsDirectory);
const execute = promisify(execFile);
const request = async () => ({ sessions: [] });

function temporary(context: TestContext) {
    const directory = mkdtempSync(join(realpathSync(tmpdir()), "enhancer-service-"));
    context.after(() => rmSync(directory, { force: true, recursive: true }));
    return directory;
}

function cli(directory: string) {
    const binary = join(output, "bin", "codex-session-board.js");
    return (command: "serve" | "stop" | "scan") =>
        execute(process.execPath, [binary, "--state-dir", directory, command]);
}

function activateFixture(directory: string, store: string, changed: boolean) {
    const runtime = join(directory, "dist");
    cpSync(output, runtime, { recursive: true });
    if (changed) appendFileSync(join(runtime, "compat", "server.js"), "\n");
    const version = "0.3.999-test";
    writeFileSync(join(runtime, "board-assets", "version.txt"), `${version}\n`);
    const manifest = join(directory, releaseManifestName);
    writeFileSync(manifest, JSON.stringify(versionManifest(runtime, version, releasePlatform())));
    activateRelease(directory, store);
    const server = join(store, sha256File(manifest), "dist", "compat", "server.js");
    const runtimeModule = require(server) as Pick<
        typeof import("../server.js"),
        "startBoardServer"
    >;
    return runtimeModule.startBoardServer;
}

test("CLI reuses the selected build and rejects a same-version update until the host restarts", async (context) => {
    const directory = temporary(context);
    const state = join(directory, "state");
    const boardState = join(state, "session-board");
    const releases = join(state, "releases");
    const first = activateFixture(join(directory, "first"), releases, false);
    const original = await first({ stateDir: boardState, port: 0, request });
    context.after(() => original.close());
    const run = cli(state);
    assert.equal(JSON.parse((await run("serve")).stdout).url, original.url);
    const before = readReceipt(boardState);
    const next = activateFixture(join(directory, "next"), releases, true);
    await assert.rejects(run("serve"), /其他版本/);
    await assert.rejects(run("scan"), /其他版本/);
    assert.deepEqual(readReceipt(boardState), before);
    assert.equal(JSON.parse((await run("stop")).stdout).instance, before?.instance);

    const replacement = await next({ stateDir: boardState, port: 0, request });
    context.after(() => replacement.close());
    assert.equal(JSON.parse((await run("serve")).stdout).url, replacement.url);
    assert.notEqual(readReceipt(boardState)?.service?.build, before?.service?.build);
});

test("CLI refuses a changed board configuration or port and can still stop the former service", async (context) => {
    const directory = temporary(context);
    const probe = await startBoardServer({ stateDir: directory, port: 0, request });
    const port = Number(new URL(probe.url).port);
    await probe.close();

    const file = join(directory, "config.json");
    writeFileSync(file, JSON.stringify({ port, mode: "four" }));
    const service = await startBoardServer({ stateDir: directory, request });
    context.after(() => service.close());
    assert.equal(Number(new URL(service.url).port), port);
    const run = cli(directory);
    const receipt = readReceipt(directory);
    assert.equal(JSON.parse((await run("serve")).stdout).url, service.url);
    writeFileSync(file, JSON.stringify({ mode: "four", port }, null, 4));
    assert.equal(JSON.parse((await run("serve")).stdout).url, service.url);

    writeFileSync(file, JSON.stringify({ mode: "five", port }));
    await assert.rejects(run("serve"), /配置已变化/);
    const otherPort = port === 65535 ? port - 1 : port + 1;
    writeFileSync(file, JSON.stringify({ mode: "four", port: otherPort }));
    await assert.rejects(run("serve"), /端口已变化/);
    assert.deepEqual(readReceipt(directory), receipt);
    assert.equal(JSON.parse((await run("stop")).stdout).instance, receipt?.instance);
    assert.equal(readReceipt(directory), null);
});

test("CLI rejects a copied receipt across data directories and public info omits its identity", async (context) => {
    const directory = temporary(context);
    const source = join(directory, "source");
    const other = join(directory, "other");
    const server = await startBoardServer({ stateDir: source, port: 0, request });
    context.after(() => server.close());
    const receipt = readReceipt(source);
    assert.ok(receipt?.service);
    writeReceipt(other, receipt);
    await assert.rejects(cli(other)("serve"), /其他数据目录/);
    await assert.rejects(cli(other)("stop"), /其他数据目录/);
    assert.equal(JSON.parse((await cli(source)("serve")).stdout).url, server.url);
    assert.deepEqual(readReceipt(source), receipt);
    const info = await fetch(new URL("/api/info", server.url)).then((response) => response.json());
    assert.equal(info.service, undefined);
    assert.equal(info.pid, undefined);
});
