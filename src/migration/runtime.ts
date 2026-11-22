import { createRequire } from "node:module";
import { join } from "node:path";
import type { BoardLedger } from "../extensions/session-board/archive.js";
import { type BoardSettings, readSettings } from "../extensions/session-board/settings.js";
import { readRegular, regularFile } from "./files.js";
import { projectLedger } from "./ledger.js";
import type { LegacyAccountBinding, MigratedBoard } from "./projection.js";

const requireHere = createRequire(__filename);

interface Import {
    board: MigratedBoard;
    ledger: BoardLedger;
}

interface CachedImport extends Import {
    identity: string;
}

function bindingOf(contents: string): LegacyAccountBinding {
    const binding = JSON.parse(contents) as Partial<LegacyAccountBinding> | null;
    if (
        !binding ||
        typeof binding.accountId !== "string" ||
        !binding.accountId.trim() ||
        typeof binding.userId !== "string" ||
        !binding.userId.trim() ||
        !binding.hosts ||
        typeof binding.hosts !== "object" ||
        Array.isArray(binding.hosts) ||
        Object.values(binding.hosts).some((host) => typeof host !== "string")
    ) {
        throw new Error("旧看板迁移绑定必须指明 Codex 账号、用户与主机");
    }
    return {
        accountId: binding.accountId,
        userId: binding.userId,
        hosts: binding.hosts,
    };
}

export class LegacyBoardRuntime {
    private cached: CachedImport | undefined;

    constructor(private readonly directory: string) {}

    forAccount(account: string): Import | undefined {
        const bindingFile = join(this.directory, "legacy-binding.json");
        if (!regularFile(bindingFile)) return undefined;
        const bytes = readRegular(bindingFile).toString("utf8");
        const binding = bindingOf(bytes);
        if (account !== JSON.stringify([binding.accountId, binding.userId])) return undefined;
        const destination = join(this.directory, "legacy-migration");
        const receipt = join(destination, "snapshot", "migration.json");
        if (!regularFile(receipt)) return undefined;
        const identity = JSON.stringify([bytes, readRegular(receipt).toString("utf8")]);
        if (this.cached?.identity === identity) return this.cached;
        const { readMigratedBoard } = requireHere(
            "./projection.js",
        ) as typeof import("./projection.js");
        const board = readMigratedBoard(destination, binding);
        this.cached = { identity, board, ledger: projectLedger(board, binding) };
        return this.cached;
    }

    settings(account: string | undefined, config: unknown): BoardSettings {
        if (
            config !== undefined &&
            (!config || typeof config !== "object" || Array.isArray(config))
        ) {
            return readSettings(config);
        }
        const preferences = account && this.forAccount(account)?.board.preferences;
        return readSettings({
            ...preferences,
            ...config,
        });
    }
}
