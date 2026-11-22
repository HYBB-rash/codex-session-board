import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
    cpSync,
    existsSync,
    mkdirSync,
    mkdtempSync,
    realpathSync,
    rmSync,
    symlinkSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import type { InstallState } from "./install.js";
import { outputDir } from "./paths.js";

const mac = process.platform === "darwin";

function scratch(context: TestContext): string {
    const root = mkdtempSync(path.join(realpathSync(tmpdir()), "enhancer-launch-"));
    context.after(() => rmSync(root, { recursive: true, force: true }));
    return root;
}

function fixtureApp(app: string) {
    const resources = path.join(app, mac ? "Contents/Resources" : "resources");
    mkdirSync(resources, { recursive: true });
    const launcher = path.join(app, mac ? "Contents/MacOS/FakeCodex" : "start.sh");
    mkdirSync(path.dirname(launcher), { recursive: true });
    if (mac) {
        writeFileSync(
            path.join(app, "Contents", "Info.plist"),
            '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>' +
                "<key>CFBundleExecutable</key><string>FakeCodex</string></dict></plist>",
        );
    }
    const script = [
        "#!/usr/bin/env node",
        "const env = process.env;",
        "console.log(JSON.stringify({ cwd: process.cwd(), args: process.argv.slice(2),",
        "cli: env.CODEX_CLI_PATH, socket: env.CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET,",
        "appId: env.CODEX_LINUX_APP_ID, state: env.CODEX_ENHANCER_STATE_DIR,",
        "profile: env.CODEX_ELECTRON_USER_DATA_PATH, home: env.CODEX_HOME,",
        "config: env.XDG_CONFIG_HOME }));",
    ].join("\n");
    writeFileSync(launcher, `${script}\n`, { mode: 0o700 });
    return resources;
}

function fixtureInstallation(root: string, source: string) {
    const stateDir = path.join(root, "state");
    const app = path.join(stateDir, mac ? "Codex Enhancer.app" : "test-app");
    const resources = fixtureApp(app);
    const dist = path.join(root, "dist");
    cpSync(path.join(outputDir, "installer"), path.join(dist, "installer"), { recursive: true });
    const binary = path.join(dist, "bin", "codex-enhancer.js");
    mkdirSync(path.dirname(binary));
    cpSync(path.join(outputDir, "bin", "codex-enhancer.js"), binary);
    const entry = { codexVersion: "fixture", runtimeType: null, chromiumVersion: "fixture" };
    const archive = path.relative(app, path.join(resources, "app.asar"));
    const receipt: InstallState = {
        schema: 1,
        status: "installed",
        patchVersion: "fixture",
        installedAt: "fixture",
        source: { path: source, ...entry, rpm: null },
        target: { kind: "test-copy", path: app },
        app: { ...entry, archive, entry: "main.js" },
        files: { archive: { path: archive, originalSha256: "fixture", patchedSha256: "fixture" } },
        backup: { archive: "fixture" },
        loader: { sentinel: "fixture", link: "fixture", runtime: "fixture" },
    };
    writeFileSync(path.join(stateDir, "install.json"), JSON.stringify(receipt));
    return { app, binary, dist, resources, stateDir };
}

function fixtureEnvironment(cwd: string): NodeJS.ProcessEnv {
    const { PATH } = process.env;
    const env = {
        ...process.env,
        PATH: [path.dirname(process.execPath), PATH].filter(Boolean).join(path.delimiter),
        CODEX_ENHANCER_LOG_STDOUT: "0",
        XDG_RUNTIME_DIR: path.join(cwd, "runtime"),
    };
    delete env.CODEX_CLI_PATH;
    delete env.CODEX_ENHANCER_BRIDGE_SOCKET;
    delete env.CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET;
    delete env.CODEX_LINUX_APP_ID;
    return env;
}

interface LauncherOutput {
    appId?: string;
    args: string[];
    cli?: string;
    config?: string;
    cwd: string;
    home: string;
    profile: string;
    socket?: string;
    state: string;
}

function runLauncher(
    installed: ReturnType<typeof fixtureInstallation>,
    cwd: string,
    env: NodeJS.ProcessEnv,
): LauncherOutput {
    const stdout = execFileSync(process.execPath, [installed.binary, "launch", "--fixture"], {
        cwd,
        env,
        encoding: "utf8",
        timeout: 5000,
    });
    const output = stdout.split("\n").find((line) => line.startsWith("{"));
    assert.ok(output);
    const observed = JSON.parse(output) as LauncherOutput;
    const profile = path.join(installed.stateDir, "user-data");
    assert.equal(observed.cwd, installed.app);
    assert.deepEqual(observed.args, [`--user-data-dir=${profile}`, "--fixture"]);
    assert.equal(observed.profile, profile);
    assert.equal(observed.home, path.join(installed.stateDir, "codex-home"));
    assert.equal(observed.state, installed.stateDir);
    return observed;
}

test("relocated installations use their own Codex profile and desktop socket", (context) => {
    const cwd = scratch(context);
    const source = path.join(cwd, mac ? "Stock Codex.app" : "stock");
    fixtureApp(source);
    const env = fixtureEnvironment(cwd);
    const first = fixtureInstallation(path.join(cwd, "first deployment"), source);
    const second = fixtureInstallation(path.join(cwd, "second deployment"), source);
    const outputs = [first, second].map((installed) => runLauncher(installed, cwd, env));
    if (mac) {
        assert.ok(
            outputs.every((output) => output.cli === undefined && output.socket === undefined),
        );
        return;
    }
    assert.notEqual(outputs[0]?.appId, outputs[1]?.appId);
    for (const [index, installed] of [first, second].entries()) {
        const output = outputs[index];
        assert.ok(output?.appId && env.XDG_RUNTIME_DIR);
        const directory = path.join(env.XDG_RUNTIME_DIR, output.appId, "app-server-bridge");
        assert.equal(output.socket, path.join(directory, "app-server.sock"));
        assert.ok(existsSync(directory));
        assert.equal(output.cli, path.join(installed.resources, "codex"));
        assert.equal(output.config, path.join(installed.stateDir, "xdg-config"));
    }
});

test("the installer and launcher resolve configured source, command and socket paths", (context) => {
    const cwd = scratch(context);
    const source = path.join(cwd, mac ? "Stock Codex.app" : "stock");
    fixtureApp(source);
    const alias = path.join(cwd, "selected-codex");
    symlinkSync(source, alias);
    const installed = fixtureInstallation(path.join(cwd, "deployment"), source);
    const env = {
        ...fixtureEnvironment(cwd),
        CODEX_ENHANCER_SOURCE: path.basename(alias),
        CODEX_CLI_PATH: "commands/custom codex",
        CODEX_ENHANCER_BRIDGE_SOCKET: "custom/desktop.sock",
    };
    const appSource = execFileSync(
        process.execPath,
        [
            "-e",
            "console.log(require(process.argv[1]).findSystemApp().appDir)",
            path.join(installed.dist, "installer", "detect.js"),
        ],
        { cwd, env, encoding: "utf8" },
    );
    assert.equal(appSource.trim(), source);
    const output = runLauncher(installed, cwd, env);
    assert.equal(output.cli, path.join(cwd, env.CODEX_CLI_PATH));
    assert.equal(output.socket, mac ? undefined : path.join(cwd, env.CODEX_ENHANCER_BRIDGE_SOCKET));
});
