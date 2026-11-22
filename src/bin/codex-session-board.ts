#!/usr/bin/env node

import { mkdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { loadCliConfig, migrateSections } from "../compat/cli-config.js";
import { connectBoard } from "../compat/client.js";
import { serveMcp } from "../compat/mcp.js";
import { assetsDirectory, object } from "../compat/operations.js";
import { stateDir as defaultStateDir } from "../installer/paths.js";
import { migrateLegacyBoard, rollbackLegacyMigration } from "../migration/legacy.js";

const commands = "serve|stop|mcp|scan|migrate|migrate-sections|check-sections";
const options = {
    help: { type: "boolean", short: "h" },
    version: { type: "boolean", short: "V" },
    config: { type: "string" },
    "state-dir": { type: "string" },
    "source-config": { type: "string" },
    output: { type: "string" },
    rollback: { type: "string" },
} as const;

function version(): string {
    return readFileSync(join(assetsDirectory, "version.txt"), "utf8").trim();
}

function checkSections(value: unknown) {
    const data = object<"health" | "states" | "offline">(value);
    const health = object<"nativeVerified" | "sectionBindings" | "nativeError">(data?.health);
    const states = object(data?.states);
    if (!states || data?.offline === true || health?.nativeVerified !== true) {
        throw new Error(
            typeof health?.nativeError === "string"
                ? health.nativeError
                : "当前 Codex 窗口尚未确认本机原生分区",
        );
    }
    const bindings = Array.isArray(health.sectionBindings) ? health.sectionBindings : [];
    const sectionIds = new Set<string>();
    const ids = Object.entries(states).map(([state, label], index) => {
        const binding = object<"state" | "label" | "sectionId">(bindings[index]);
        if (
            binding?.state !== state ||
            binding.label !== label ||
            typeof binding.sectionId !== "string" ||
            !binding.sectionId.trim() ||
            sectionIds.has(binding.sectionId)
        ) {
            throw new Error("当前 Codex 窗口返回的本机原生分区绑定无效");
        }
        sectionIds.add(binding.sectionId);
        return { state, label, sectionId: binding.sectionId };
    });
    if (!ids.length || ids.length !== bindings.length)
        throw new Error("当前 Codex 窗口返回的本机原生分区数无效");
    return {
        app: "codex-session-board",
        version: version(),
        hostId: "local",
        nativeCatalogVerified: true,
        bindings: ids,
    };
}

function reply(result: unknown) {
    console.log(JSON.stringify(result));
}

async function execute(
    args: ReturnType<typeof parseArgs<{ options: typeof options; allowPositionals: true }>>,
) {
    const { positionals, values } = args;
    const command = positionals[0] ?? "serve";
    const { CODEX_SESSION_BOARD_CONFIG, CODEX_ENHANCER_STATE_DIR } = process.env;
    const config = values.config ?? CODEX_SESSION_BOARD_CONFIG;
    const configFile = config ? resolve(config) : undefined;
    const configured = command === "migrate-sections" ? {} : loadCliConfig(configFile);
    const stateDir = resolve(
        values["state-dir"] ?? configured.stateDir ?? CODEX_ENHANCER_STATE_DIR ?? defaultStateDir,
    );
    if (command === "migrate") {
        const location = join(stateDir, "session-board");
        const destination = resolve(values.output ?? join(location, "legacy-migration"));
        if (values.rollback) {
            if (values["source-config"]) throw new Error("迁移回退与源导入必须分开执行");
            reply({ backup: rollbackLegacyMigration(destination, values.rollback) });
            return;
        }
        if (!values["source-config"]) throw new Error("migrate 要求 --source-config");
        mkdirSync(location, { recursive: true, mode: 0o700 });
        reply(
            await migrateLegacyBoard({
                sourceConfig: resolve(values["source-config"]),
                destination,
            }),
        );
        return;
    }
    if (command === "migrate-sections") {
        if (!configFile || !values.output)
            throw new Error("migrate-sections 要求 --config 和 --output");
        reply(migrateSections(configFile, resolve(values.output)));
        return;
    }
    if (command === "mcp") {
        const request = async (path: string, body?: object) =>
            (await connectBoard(stateDir)).request(path, body);
        await serveMcp(
            { request, version: version(), legacy: configured.legacy ?? false },
            process.stdin,
            process.stdout,
        );
        return;
    }
    await dispatchService(command, stateDir, configFile);
}

async function dispatchService(command: string, stateDir: string, config: string | undefined) {
    if (!["serve", "stop", "scan", "check-sections"].includes(command))
        throw new Error(`未知看板命令 ${command}`);
    const service = await connectBoard(stateDir);
    switch (command) {
        case "serve":
            reply({ url: service.url, running: true, started: false, config: config ?? null });
            break;
        case "stop":
            reply(await service.stop());
            break;
        case "scan":
            reply(await service.request("/api/sessions"));
            break;
        case "check-sections":
            reply(checkSections(await service.request("/api/sessions")));
            break;
    }
}

async function main() {
    const args = parseArgs({ options, allowPositionals: true });
    if (args.values.help)
        console.log(
            `codex-session-board [--config 文件] [--state-dir 目录] ${commands}\n连接到已启动的 Codex Enhancer 看板。`,
        );
    else if (args.values.version) console.log(`codex-session-board ${version()}`);
    else {
        if (args.positionals.length > 1) throw new Error("仅接受一个看板命令");
        await execute(args);
    }
}

void main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
});
