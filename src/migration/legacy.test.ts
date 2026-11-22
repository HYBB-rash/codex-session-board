import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
    existsSync,
    mkdirSync,
    mkdtempSync,
    readdirSync,
    readFileSync,
    rmSync,
    statSync,
    symlinkSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { BoardLedgers, BoardRecords } from "../extensions/session-board/records.js";
import {
    migrateLegacyBoard,
    readLegacyMigration,
    rollbackLegacyMigration,
    verifyLegacyMigration,
} from "./legacy.js";
import { readMigratedBoard } from "./projection.js";
import { LegacyBoardRuntime } from "./runtime.js";

const accountId = "workspace";
const userId = "user";
const sourceRemote = "remote:original";
const sourceOffline = "remote:offline";
const installedRemote = "remote-ssh-discovered:original";
const threadId = "019ba3a3-ca32-7000-8000-d2079233e205";
const remoteDirectory = (host: string) => createHash("sha256").update(host).digest("hex");

const fixtureSchema = `
    CREATE TABLE sessions (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, cwd TEXT NOT NULL, updated_at INTEGER NOT NULL,
        note TEXT NOT NULL, selected_state TEXT, version INTEGER NOT NULL, input_revision INTEGER NOT NULL,
        selection_turn TEXT, selection_users TEXT, selected_at REAL, needs_review INTEGER NOT NULL,
        native_state TEXT, native_section TEXT, runtime_pending_key TEXT, archived INTEGER NOT NULL,
        turn_json TEXT
    );
    CREATE TABLE changes (thread_id TEXT, changed_at REAL, state TEXT, note TEXT);
    CREATE TABLE title_updates (thread_id TEXT, version INTEGER, state TEXT, requested_at REAL, error TEXT);
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
    CREATE TABLE archive_batches (id TEXT PRIMARY KEY, created_at REAL, finished_at REAL);
    CREATE TABLE archive_batch_items (batch_id TEXT, position INTEGER, id TEXT, target TEXT,
        status TEXT, reason TEXT, source TEXT, checked_at REAL);
    CREATE TABLE remote_sessions (id TEXT, host_id TEXT, thread_id TEXT, title TEXT, cwd TEXT,
        updated_at INTEGER, state TEXT, note TEXT, version INTEGER, input_version INTEGER);
    CREATE TABLE remote_projects (id TEXT, value TEXT);
`;

function seed(database: DatabaseSync, host: string): void {
    database.exec(fixtureSchema);
    database.exec("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0");
    database
        .prepare(`INSERT INTO sessions VALUES
        (?, 'fixture', '/fixture', 41, ?, 'to_archive', 12, 7, 'turn', '["msg-a","msg-b"]', 10, 1,
        'to_archive', 'old-replica', 'pending-phase', 0,
        '{"id":"turn","userItems":["msg-a","msg-b","msg-c"],"hasFinal":true}')`)
        .run(threadId, `${host} WAL note`);
    database
        .prepare("INSERT INTO changes VALUES (?, 9, 'review', ?)")
        .run(threadId, `${host} history`);
    database
        .prepare("INSERT INTO title_updates VALUES (?, 12, 'to_archive', 10, ?)")
        .run(threadId, `${host} title retry`);
    database
        .prepare("INSERT INTO settings VALUES (?, ?)")
        .run("column_sort:to_archive", '{"field":"updated_at","direction":"desc","version":4}');
    database
        .prepare("INSERT INTO settings VALUES (?, ?)")
        .run(
            "draft_request:request",
            '{"requestId":"request","stage":"starting","project":"default"}',
        );
    database.prepare("INSERT INTO settings VALUES (?, ?)").run("draft_pending:default", "request");
    database
        .prepare("INSERT INTO settings VALUES (?, ?)")
        .run("native_draft_v1:request", '{"operationId":"request","stage":"prepared"}');
    database.exec("INSERT INTO archive_batches VALUES ('batch', 10, NULL)");
    database
        .prepare(
            "INSERT INTO archive_batch_items VALUES ('batch', 0, ?, ?, 'pending', NULL, ?, NULL)",
        )
        .run(
            threadId,
            JSON.stringify({
                id: threadId,
                title: "fixture",
                version: 12,
                inputVersion: 7,
                sectionVersion: "old-replica",
            }),
            JSON.stringify({ home: "/fixture", account: "old-account" }),
        );
}

function scopeFixture() {
    const base = mkdtempSync(join(tmpdir(), "board-migration-test-"));
    const source = join(base, "old");
    const sourceConfig = join(base, "source.json");
    const destination = join(base, "new");
    const roots = [
        source,
        ...[sourceRemote, sourceOffline].map((host) =>
            join(source, "remote-hosts", remoteDirectory(host)),
        ),
    ];
    for (const root of roots) mkdirSync(root, { recursive: true });
    writeFixtureConfig(sourceConfig);
    const databases = roots.map((root) => new DatabaseSync(join(root, "status.sqlite3")));
    for (const [position, host] of ["local", sourceRemote, sourceOffline].entries()) {
        const database = databases[position];
        if (!database) throw new Error("Fixture database missing");
        seed(database, host);
        if (position !== 0) {
            const folder = join(source, "remote-hosts", remoteDirectory(host));
            writeFileSync(
                join(folder, "source.json"),
                JSON.stringify({ schemaVersion: 2, hostId: host }),
            );
            writeFileSync(join(folder, "scan-progress.json"), '{"cursor":"next"}');
        }
    }
    writeFileSync(join(source, "server.json"), '{"pid":101}');
    writeFileSync(join(source, "credentials.json"), "{}");
    return {
        base,
        source,
        destination,
        sourceConfig,
        roots,
        databases,
        close() {
            for (const database of databases) database.close();
            rmSync(base, { recursive: true, force: true });
        },
    };
}

function writeFixtureConfig(sourceConfig: string): void {
    writeFileSync(
        sourceConfig,
        JSON.stringify({
            data_dir: "./old",
            four_columns: true,
            sync_titles: true,
            auto_archive_seconds: 60,
            remote_auto_archive_seconds: 120,
        }),
    );
}

function dump(database: DatabaseSync) {
    return [
        "sessions",
        "changes",
        "title_updates",
        "settings",
        "archive_batches",
        "archive_batch_items",
        "remote_sessions",
        "remote_projects",
    ].map((table) => database.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all());
}

function filePair(directory: string) {
    return ["status.sqlite3", "status.sqlite3-wal"].map((name) =>
        readFileSync(join(directory, name)),
    );
}

function compareDatabases(fixture: ReturnType<typeof scopeFixture>, destination: string) {
    for (const [index, root] of fixture.roots.entries()) {
        const relativeDirectory = root.slice(fixture.source.length);
        const file = join(destination, relativeDirectory, "status.sqlite3");
        const database = new DatabaseSync(file, { readOnly: true });
        try {
            const source = fixture.databases[index];
            assert.ok(source);
            assert.deepEqual(dump(database), dump(source));
            assert.equal(statSync(file).mode & 0o777, 0o600);
            assert.equal(existsSync(`${file}-wal`), false);
        } finally {
            database.close();
        }
    }
}

test("live committed WAL from all hosts is backed up without reading runtime files", async () => {
    const fixture = scopeFixture();
    try {
        const originals = fixture.roots.map(filePair);
        const result = await migrateLegacyBoard(fixture);
        assert.ok(result.receipt.mainDatabase);
        assert.equal(result.receipt.remoteHosts.length, 2);
        verifyLegacyMigration(readLegacyMigration(fixture.destination));
        compareDatabases(fixture, result.directory);
        assert.deepEqual(fixture.roots.map(filePair), originals);
        assert.deepEqual(
            readFileSync(join(result.directory, "config.json")),
            readFileSync(fixture.sourceConfig),
        );
        assert.deepEqual(readdirSync(result.directory).sort(), [
            "config.json",
            "migration.json",
            "remote-hosts",
            "status.sqlite3",
        ]);
        const offline = join(result.directory, "remote-hosts", remoteDirectory(sourceOffline));
        assert.equal(
            readFileSync(join(offline, "scan-progress.json"), "utf8"),
            '{"cursor":"next"}',
        );
        assert.equal(statSync(join(offline, "source.json")).mode & 0o777, 0o600);
    } finally {
        fixture.close();
    }
});

test("projection retains revisions, pending work, and same thread IDs per explicitly bound host", async () => {
    const fixture = scopeFixture();
    try {
        await migrateLegacyBoard(fixture);
        const board = readMigratedBoard(fixture.destination, {
            accountId,
            userId,
            hosts: { local: "local", [sourceRemote]: installedRemote },
        });
        const local = board.cards[threadId];
        const remote = board.cards[JSON.stringify([installedRemote, threadId])];
        assert.equal(board.accountKey, JSON.stringify([accountId, userId]));
        assert.equal(local?.note, "local WAL note");
        assert.equal(remote?.note, `${sourceRemote} WAL note`);
        assert.equal(remote?.sourceHostId, sourceRemote);
        assert.equal(remote?.manual, true);
        assert.equal(remote?.followUp, true);
        assert.equal(remote?.inputVersion, 7);
        assert.equal(remote?.version, 12);
        assert.equal(remote?.inputKey, '["turn",["msg-a","msg-b","msg-c"]]');
        assert.equal(remote?.confirmedInputKey, '["turn",["msg-a","msg-b"]]');
        assert.equal(remote?.previewTurnId, "turn");
        assert.equal(remote?.title?.error, `${sourceRemote} title retry`);
        assert.deepEqual(remote?.selectionUsers, ["msg-a", "msg-b"]);
        assert.equal(remote?.runtimePendingKey, "pending-phase");
        assert.equal(remote?.pendingPhaseKey, "pending-phase");
        assert.equal(board.preferences.remoteAutoArchiveSeconds, 120);
        assert.equal(board.preferences.mode, "four");
        assert.deepEqual(board.columnSorts.to_archive, {
            field: "updated_at",
            direction: "desc",
            version: 4,
        });
        assert.deepEqual(board.unboundHostDirectories, [remoteDirectory(sourceOffline)]);
        const state = board.databases.find((entry) => entry.hostId === installedRemote);
        assert.equal(state?.titleUpdates[0]?.error, `${sourceRemote} title retry`);
        assert.equal(state?.archiveItems[0]?.status, "pending");
        assert.equal(state?.settings["draft_pending:default"], "request");
        assert.ok(state?.settings["native_draft_v1:request"]?.includes("prepared"));
    } finally {
        fixture.close();
    }
});

const storedAccount = JSON.stringify([accountId, userId]);
const installedId = JSON.stringify([installedRemote, threadId]);
const installedOffline = "remote-ssh-discovered:offline";
const offlineId = JSON.stringify([installedOffline, threadId]);
const archiveColumn = "to_archive";

function runtimeFixture(fixture: ReturnType<typeof scopeFixture>) {
    const directory = join(fixture.base, "runtime");
    const destination = join(directory, "legacy-migration");
    const bindingFile = join(directory, "legacy-binding.json");
    const recordsFile = join(directory, "records.json");
    const ledgersFile = join(directory, "ledger.json");
    const retiredRemote = `remote:${JSON.stringify(["codex", sourceRemote, threadId])}`;
    mkdirSync(directory);
    const main = fixture.databases[0];
    assert.ok(main);
    main.prepare(
        "INSERT INTO archive_batch_items VALUES ('batch', 1, ?, ?, 'awaiting', NULL, NULL, NULL)",
    ).run(
        retiredRemote,
        JSON.stringify({
            id: retiredRemote,
            title: "remote",
            version: 12,
            inputVersion: 7,
            sectionVersion: null,
        }),
    );
    return {
        directory,
        destination,
        recordsFile,
        ledgersFile,
        migration: new LegacyBoardRuntime(directory),
        records: new BoardRecords(recordsFile),
        ledgers: new BoardLedgers(ledgersFile),
        writeBinding: (hosts: Record<string, string>) =>
            writeFileSync(bindingFile, JSON.stringify({ accountId, userId, hosts })),
    };
}

function assertSeeded(runtime: ReturnType<typeof runtimeFixture>) {
    const { migration, records, ledgers, recordsFile, ledgersFile } = runtime;
    const unrelated = JSON.stringify([accountId, "different-user"]);
    const seed = migration.forAccount(storedAccount);
    assert.ok(seed);
    assert.equal(migration.forAccount(unrelated), undefined);
    const seeded = records.account(storedAccount, seed.board.cards);
    assert.equal(seeded[threadId]?.note, "local WAL note");
    assert.equal(seeded[installedId]?.title?.state, "to_archive");
    assert.equal(seeded[offlineId], undefined);
    assert.deepEqual(records.account(unrelated), {});
    const settings = migration.settings(storedAccount, { syncTitles: false });
    assert.equal(settings.syncTitles, false);
    assert.equal(settings.remoteAutoArchiveSeconds, 120);
    assert.equal(migration.settings(unrelated, {}).remoteAutoArchiveSeconds, null);
    assert.equal(existsSync(recordsFile), false);
    assert.equal(existsSync(ledgersFile), false);
    const existing = ledgers.load(storedAccount, seed.ledger);
    assert.equal(existing.batches.length, 2);
    assert.deepEqual(
        existing.batches
            .find(({ id }) => id === "batch")
            ?.items.map(({ id, status }) => [id, status]),
        [
            [threadId, "pending"],
            [installedId, "awaiting"],
        ],
    );
    assert.equal(existing.columnSorts[archiveColumn]?.version, 4);
    assert.ok(
        existing.pendingDrafts?.some(
            ({ project, stage }) => project === "default" && stage === "starting",
        ),
    );
    return seed;
}

function editMigrated(
    runtime: ReturnType<typeof runtimeFixture>,
    seed: ReturnType<typeof assertSeeded>,
) {
    const { records, ledgers } = runtime;
    const update = {
        account: storedAccount,
        id: threadId,
        expected: 12,
        record: { note: "new", version: 13, inputVersion: 7 },
    };
    records.write(update, seed.board.cards);
    assert.throws(() => records.write(update, seed.board.cards), /已更新/);
    const existing = ledgers.load(storedAccount, seed.ledger);
    ledgers.save(
        storedAccount,
        {
            ...existing,
            pendingDrafts: (existing.pendingDrafts ?? []).map((draft) => ({
                ...draft,
                ...(draft.project === "default" ? { stage: "abandoned" } : {}),
            })),
            columnSorts: {
                ...existing.columnSorts,
                to_archive: { field: "updated_at", direction: "asc", version: 5 },
            },
        },
        seed.ledger,
    );
}

test("runtime overlays explicitly bound records, preferences and batches without overwriting saved state", async () => {
    const fixture = scopeFixture();
    try {
        const runtime = runtimeFixture(fixture);
        const { migration, records, ledgers, recordsFile, ledgersFile, directory, destination } =
            runtime;
        const imported = await migrateLegacyBoard({ ...fixture, destination });
        assert.equal(migration.forAccount(storedAccount), undefined);
        runtime.writeBinding({ local: "local", [sourceRemote]: installedRemote });
        editMigrated(runtime, assertSeeded(runtime));
        runtime.writeBinding({
            local: "local",
            [sourceRemote]: installedRemote,
            [sourceOffline]: installedOffline,
        });
        const widened = new LegacyBoardRuntime(directory).forAccount(storedAccount);
        assert.ok(widened);
        const reopened = new BoardRecords(recordsFile).account(storedAccount, widened.board.cards);
        assert.equal(reopened[threadId]?.note, "new");
        assert.equal(reopened[offlineId]?.note, `${sourceOffline} WAL note`);
        const updatedLedger = new BoardLedgers(ledgersFile).load(storedAccount, widened.ledger);
        assert.equal(updatedLedger.batches.length, 3);
        assert.equal(updatedLedger.columnSorts[archiveColumn]?.version, 5);
        assert.equal(
            updatedLedger.pendingDrafts?.find(({ project }) => project === "default")?.stage,
            "abandoned",
        );
        assert.ok(
            updatedLedger.pendingDrafts?.some(
                ({ project }) =>
                    project ===
                    `target:${JSON.stringify([installedOffline, "targetServer", "default"])}`,
            ),
        );
        rollbackLegacyMigration(destination, imported.receipt.id);
        assert.equal(migration.forAccount(storedAccount), undefined);
        assert.equal(records.account(storedAccount)[threadId]?.note, "new");
        assert.equal(records.account(storedAccount)[offlineId], undefined);
        const rolledBack = ledgers.load(storedAccount);
        assert.equal(rolledBack.columnSorts[archiveColumn]?.version, 5);
        assert.equal(rolledBack.batches.length, 0);
    } finally {
        fixture.close();
    }
});

test("migration refuses occupied destinations, rolls back privately, and can retry", async () => {
    const fixture = scopeFixture();
    try {
        mkdirSync(fixture.destination);
        await assert.rejects(migrateLegacyBoard(fixture), /已经存在/);
        assert.deepEqual(readdirSync(fixture.destination), []);
        rmSync(fixture.destination, { recursive: true });
        const before = await migrateLegacyBoard(fixture);
        await assert.rejects(migrateLegacyBoard(fixture), /已经存在/);
        assert.throws(
            () => rollbackLegacyMigration(fixture.destination, "different"),
            /编号不匹配/,
        );
        const retained = rollbackLegacyMigration(fixture.destination, before.receipt.id);
        assert.equal(existsSync(fixture.destination), false);
        assert.equal(readLegacyMigration(retained).receipt.id, before.receipt.id);
        const after = await migrateLegacyBoard(fixture);
        verifyLegacyMigration(after);
        assert.notEqual(after.receipt.id, before.receipt.id);
        compareDatabases(fixture, after.directory);
    } finally {
        fixture.close();
    }
});

test("late corrupt remote snapshot leaves no published state and a corrected source retries", async () => {
    const fixture = scopeFixture();
    try {
        const broken = join(fixture.source, "remote-hosts", remoteDirectory("remote:corrupt"));
        mkdirSync(broken);
        writeFileSync(join(broken, "status.sqlite3"), "not a database");
        await assert.rejects(migrateLegacyBoard(fixture));
        assert.equal(existsSync(fixture.destination), false);
        assert.deepEqual(readdirSync(fixture.base).sort(), ["old", "source.json"]);
        rmSync(broken, { recursive: true });
        const result = await migrateLegacyBoard(fixture);
        compareDatabases(fixture, result.directory);
    } finally {
        fixture.close();
    }
});

test("concurrent migrations publish exactly one complete snapshot", async () => {
    const fixture = scopeFixture();
    try {
        const results = await Promise.allSettled([
            migrateLegacyBoard(fixture),
            migrateLegacyBoard(fixture),
        ]);
        assert.equal(results.filter(({ status }) => status === "fulfilled").length, 1);
        assert.equal(results.filter(({ status }) => status === "rejected").length, 1);
        assert.deepEqual(readdirSync(fixture.base).sort(), ["new", "old", "source.json"]);
        verifyLegacyMigration(readLegacyMigration(fixture.destination));
    } finally {
        fixture.close();
    }
});

test("a live source writer does not leak uncommitted data into the snapshot", async () => {
    const fixture = scopeFixture();
    const writer = fixture.databases[0];
    assert.ok(writer);
    try {
        writer.exec("BEGIN IMMEDIATE; UPDATE sessions SET note='uncommitted';");
        const result = await migrateLegacyBoard(fixture);
        const board = readMigratedBoard(fixture.destination, {
            accountId,
            userId,
            hosts: { local: "local" },
        });
        assert.equal(board.cards[threadId]?.note, "local WAL note");
        assert.equal(result.receipt.mainDatabase, true);
        writer.exec("ROLLBACK");
    } finally {
        fixture.close();
    }
});

test("databases for offline hosts survive if the main database is missing", async () => {
    const fixture = scopeFixture();
    try {
        const writer = fixture.databases.shift();
        assert.ok(writer);
        writer.close();
        rmSync(join(fixture.source, "status.sqlite3"));
        const result = await migrateLegacyBoard(fixture);
        assert.equal(result.receipt.mainDatabase, false);
        assert.equal(result.receipt.remoteHosts.length, 2);
        const board = readMigratedBoard(fixture.destination, {
            accountId,
            userId,
            hosts: { [sourceRemote]: installedRemote },
        });
        assert.equal(
            board.cards[JSON.stringify([installedRemote, threadId])]?.note,
            `${sourceRemote} WAL note`,
        );
        assert.deepEqual(board.unboundHostDirectories, [remoteDirectory(sourceOffline)]);
    } finally {
        fixture.close();
    }
});

test("an explicit source alias resolves while a SQLite sidecar alias is refused", async () => {
    const fixture = scopeFixture();
    try {
        const alias = join(fixture.base, "config-alias");
        const external = join(fixture.base, "external");
        writeFileSync(external, "unchanged");
        symlinkSync(fixture.sourceConfig, alias);
        const sourceJournal = join(fixture.source, "status.sqlite3-journal");
        symlinkSync(external, sourceJournal);
        const options = { sourceConfig: alias, destination: fixture.destination };
        await assert.rejects(migrateLegacyBoard(options), /符号链接/);
        assert.equal(readFileSync(external, "utf8"), "unchanged");
        assert.equal(existsSync(fixture.destination), false);
        rmSync(sourceJournal);
        verifyLegacyMigration(await migrateLegacyBoard(options));
    } finally {
        fixture.close();
    }
});

function createHotJournal(file: string): void {
    const database = new DatabaseSync(file);
    database.exec(`CREATE TABLE records (value TEXT, payload BLOB);
        WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM nums WHERE n < 128)
        INSERT INTO records SELECT 'committed', zeroblob(3000) FROM nums`);
    database.close();
    const child = spawnSync(process.execPath, [
        "-e",
        `const {DatabaseSync}=require('node:sqlite');
        const database=new DatabaseSync(process.argv[1]);
        database.exec("PRAGMA cache_size=1; BEGIN IMMEDIATE; UPDATE records SET value='uncommitted',payload=zeroblob(3000)");
        process.exit(0);`,
        file,
    ]);
    assert.equal(child.status, 0);
    assert.ok(statSync(`${file}-journal`).size > 512);
}

test("a hot rollback journal is recovered on a private copy", {
    skip: process.platform !== "linux",
}, async () => {
    const base = mkdtempSync(join(tmpdir(), "board-hot-journal-"));
    try {
        const source = join(base, "source");
        mkdirSync(source);
        const sourceConfig = join(base, "source.json");
        writeFileSync(sourceConfig, '{"data_dir":"./source"}');
        const file = join(source, "status.sqlite3");
        createHotJournal(file);
        const original = [readFileSync(file), readFileSync(`${file}-journal`)];
        const result = await migrateLegacyBoard({
            sourceConfig,
            destination: join(base, "destination"),
        });
        const copy = new DatabaseSync(join(result.directory, "status.sqlite3"), { readOnly: true });
        try {
            assert.equal(
                Object.values(
                    copy.prepare("SELECT count(*) FROM records WHERE value='committed'").get() ??
                        {},
                )[0],
                128,
            );
        } finally {
            copy.close();
        }
        assert.deepEqual([readFileSync(file), readFileSync(`${file}-journal`)], original);
    } finally {
        rmSync(base, { recursive: true, force: true });
    }
});
