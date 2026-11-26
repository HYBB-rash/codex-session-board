import * as fs from "node:fs";
import * as path from "node:path";
import type { BoardLedger } from "../../extensions/archive.js";
import {
    createBoardLedgers,
    createBoardRecords,
    createTaskCardCache,
    type PhaseIntent,
    type RecordWrite,
    startTitleEpoch,
    type TitleResolution,
} from "../../extensions/records.js";
import type { Card } from "../../extensions/service.js";
import { createLegacyBoardRuntime } from "../../migration/runtime.js";

function isFields<T extends object>(value: unknown): value is T {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return isFields<Record<string, unknown>>(value);
}

function readJson(filePath: string): unknown {
    return JSON.parse(fs.readFileSync(filePath, "utf8")) as unknown;
}

export function createBoardStorage(directory: string) {
    const records = createBoardRecords(path.join(directory, "records.json"));
    const ledgers = createBoardLedgers(path.join(directory, "ledger.json"));
    const tasks = createTaskCardCache(path.join(directory, "task-cache.json"));
    const titleEpochFile = path.join(directory, "title-activation.json");
    const legacy = createLegacyBoardRuntime(directory);

    function taskCache(account: string, cards?: unknown): unknown {
        if (cards === undefined) return tasks.read(account);
        if (!Array.isArray(cards)) throw new Error("Invalid task cache");
        tasks.save(account, cards as Card[]);
        return undefined;
    }

    function ledger(account: string, value?: unknown): unknown {
        const imported = legacy.forAccount(account)?.ledger;
        if (value === undefined) return ledgers.load(account, imported);
        if (
            !isFields<BoardLedger>(value) ||
            !Array.isArray(value.batches) ||
            !isRecord(value.columnSorts)
        )
            throw new Error("Invalid board ledger");
        ledgers.save(account, value, imported);
        return null;
    }

    function config(account: string): unknown {
        const file = path.join(directory, "config.json");
        return legacy.settings(account, fs.existsSync(file) ? readJson(file) : undefined);
    }

    function titleEpoch(account: string): number {
        return startTitleEpoch(titleEpochFile, account, legacy.titleEpoch(account));
    }

    function phaseIntent(value: unknown): unknown {
        if (
            !isFields<PhaseIntent>(value) ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            typeof value.phaseKey !== "string"
        )
            throw new Error("Invalid board phase intent");
        return records.preparePhase(value, legacy.forAccount(value.account)?.board.cards);
    }

    function record(value: unknown): unknown {
        if (typeof value === "string") return legacy.loadRecords(value, records);
        if (
            !isFields<RecordWrite>(value) ||
            typeof value.record?.note !== "string" ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            value.record.version !== value.expected + 1
        )
            throw new Error("Invalid board record");
        records.write(value, legacy.forAccount(value.account)?.board.cards);
        return null;
    }

    function titleResult(value: unknown): unknown {
        if (
            !isFields<TitleResolution>(value) ||
            typeof value.account !== "string" ||
            typeof value.id !== "string" ||
            !Number.isSafeInteger(value.expected) ||
            (value.failure !== undefined && typeof value.failure !== "string")
        )
            throw new Error("Invalid board title result");
        return records.settleTitle(value, legacy.forAccount(value.account)?.board.cards);
    }

    return { taskCache, ledger, config, titleEpoch, phaseIntent, record, titleResult };
}

export type BoardStorage = ReturnType<typeof createBoardStorage>;
