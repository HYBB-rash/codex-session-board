import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import { releaseFiles, releaseManifestName, verifyRelease } from "../installer/release-manifest.js";
import { activeRelease } from "../installer/releases.js";
import { assetsDirectory, object } from "./operations.js";

const defaultPort = 18767;
const output = dirname(assetsDirectory);

export interface ServiceIdentity {
    readonly directory: string;
    readonly build: string;
    readonly config: string;
    readonly port: number;
}

function fingerprint(text: string): string {
    return createHash("sha256").update(text).digest("hex");
}

function orderedObject(_key: string, value: unknown): unknown {
    const record = object(value);
    return record
        ? Object.fromEntries(
              Object.keys(record)
                  .sort()
                  .map((key) => [key, record[key]]),
          )
        : value;
}

function readBoardConfig(directory: string) {
    try {
        const value: unknown = JSON.parse(readFileSync(join(directory, "config.json"), "utf8"));
        const config = object<"port">(value);
        if (!config) throw new Error("看板配置必须为 JSON 对象");
        return config;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
        throw error;
    }
}

export function serviceConfiguration(directory: string): { port: number; digest: string } {
    const config = readBoardConfig(directory);
    const port = config.port === undefined ? defaultPort : config.port;
    if (typeof port !== "number" || !Number.isInteger(port) || port < 1 || port > 65535) {
        throw new Error("看板配置中的 port 必须为 1 到 65535 的整数");
    }
    return {
        port,
        digest: fingerprint(JSON.stringify({ ...config, port }, orderedObject)),
    };
}

export function serviceBuild(): string {
    const packageRoot = dirname(output);
    const build = existsSync(join(packageRoot, releaseManifestName))
        ? verifyRelease(packageRoot)
        : releaseFiles(output);
    return fingerprint(JSON.stringify(build));
}

export function expectedServiceBuild(stateRoot: string): string {
    const active = activeRelease(join(stateRoot, "releases"));
    return active ? fingerprint(JSON.stringify(active)) : serviceBuild();
}

export function readServiceIdentity(value: unknown): ServiceIdentity | null {
    const identity = object<"directory" | "build" | "config" | "port">(value);
    const sha256 = /^[a-f0-9]{64}$/;
    if (
        typeof identity?.directory !== "string" ||
        !isAbsolute(identity.directory) ||
        typeof identity.build !== "string" ||
        !sha256.test(identity.build) ||
        typeof identity.config !== "string" ||
        !sha256.test(identity.config) ||
        typeof identity.port !== "number" ||
        !Number.isInteger(identity.port) ||
        identity.port < 0 ||
        identity.port > 65535
    ) {
        return null;
    }
    const { directory, build, config, port } = identity;
    return { directory, build, config, port };
}

export function sameServiceIdentity(expected: ServiceIdentity, actual: ServiceIdentity): boolean {
    return (
        expected.directory === actual.directory &&
        expected.build === actual.build &&
        expected.config === actual.config &&
        expected.port === actual.port
    );
}
