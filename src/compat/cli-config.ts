import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { object } from "./operations.js";

export function loadCliConfig(path: string | undefined) {
    if (!path) return {};
    const raw = object<"enhancerStateDir" | "legacy_mode">(JSON.parse(readFileSync(path, "utf8")));
    if (!raw) throw new Error("看板配置必须为 JSON 对象");
    const state = raw.enhancerStateDir;
    if (state !== undefined && (typeof state !== "string" || !state.trim())) {
        throw new Error("enhancerStateDir 必须为非空路径");
    }
    return {
        stateDir: typeof state === "string" ? resolve(dirname(path), state) : undefined,
        legacy: raw.legacy_mode === true,
    };
}

export function migrateSections(source: string, output: string) {
    const input = readFileSync(source);
    const config = object<"status_sections" | "native_sections" | "legacy_mode" | "data_dir">(
        JSON.parse(input.toString("utf8")),
    );
    const sectionMap = object<"completed">(config?.status_sections);
    if (config?.native_sections !== true || config.legacy_mode === true || !sectionMap) {
        throw new Error("分区迁移要求旧原生分区配置");
    }
    const removedStates = Object.hasOwn(sectionMap, "completed") ? ["completed"] : [];
    delete sectionMap.completed;
    const dataDirectory = config.data_dir;
    if (typeof dataDirectory === "string" && dataDirectory.trim()) {
        config.data_dir = resolve(dirname(source), dataDirectory);
    }
    writeFileSync(output, `${JSON.stringify(config, null, 2)}\n`, {
        flag: "wx",
        mode: 0o600,
        flush: true,
    });
    return {
        source,
        target: output,
        removedStates,
        sourceSha256: createHash("sha256").update(input).digest("hex"),
    };
}
