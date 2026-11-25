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
import test, { type TestContext } from "node:test";
import {
    DesktopConnection,
    type SidebarContext,
    type Thread,
} from "../extensions/desktop/connection.js";
import { BoardLedgers, BoardRecords } from "../extensions/session-board/records.js";
import { BoardService } from "../extensions/session-board/service.js";
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
const legacyRemoteId = `remote:${JSON.stringify(["codex", sourceRemote, threadId])}`;
const remoteDirectory = (host: string) => createHash("sha256").update(host).digest("hex");

const fixtureSchema = `
    CREATE TABLE sessions (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, cwd TEXT NOT NULL, updated_at INTEGER NOT NULL,
        note TEXT NOT NULL, selected_state TEXT, version INTEGER NOT NULL, input_revision INTEGER NOT NULL,
        selection_turn TEXT, selection_users TEXT, selected_at REAL, needs_review INTEGER NOT NULL,
        native_state TEXT, native_section TEXT, runtime_pending_key TEXT, archived INTEGER NOT NULL,
        turn_json TEXT
    );
    CREATE TABLE changes (sequence INTEGER PRIMARY KEY, thread_id TEXT, changed_at REAL, state TEXT, note TEXT);
    CREATE TABLE title_updates (thread_id TEXT, version INTEGER, state TEXT, requested_at REAL, error TEXT);
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
    CREATE TABLE archive_batches (id TEXT PRIMARY KEY, created_at REAL, finished_at REAL);
    CREATE TABLE archive_batch_items (batch_id TEXT, position INTEGER, id TEXT, target TEXT,
        status TEXT, reason TEXT, source TEXT, checked_at REAL);
    CREATE TABLE remote_sessions (id TEXT, host_id TEXT, thread_id TEXT, title TEXT, cwd TEXT,
        updated_at INTEGER, state TEXT, note TEXT, version INTEGER, input_version INTEGER);
    CREATE TABLE remote_projects (id TEXT, value TEXT);
`;

function seedChanges(database: DatabaseSync, host: string): void {
    const append = database.prepare(
        "INSERT INTO changes (thread_id, changed_at, state, note) VALUES (?, ?, ?, ?)",
    );
    append.run(threadId, 9, "review", `${host} history`);
    append.run(threadId, 9, null, "");
    append.run(threadId, 8, "blocked", `${host} clock changed`);
    if (host === "local") {
        database
            .prepare(
                "INSERT INTO remote_sessions VALUES (?, ?, ?, 'cached', '/old', 8, 'review', 'cached', 12, 7)",
            )
            .run(legacyRemoteId, sourceRemote, threadId);
        append.run(legacyRemoteId, 8, "review", "cached remote history");
    }
}

function seed(database: DatabaseSync, host: string): void {
    database.exec(fixtureSchema);
    database.exec("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0");
    database
        .prepare(`INSERT INTO sessions VALUES
        (?, 'fixture', '/fixture', 41, ?, 'to_archive', 12, 7, 'turn', '["msg-a","msg-b"]', 10, 1,
        'to_archive', 'old-replica', 'pending-phase', 0,
        '{"id":"turn","userItems":["msg-a","msg-b","msg-c"],"hasFinal":true}')`)
        .run(threadId, `${host} WAL note`);
    seedChanges(database, host);
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
            "INSERT INTO archive_batch_items VALUES ('batch', 0, ?, ?, 'pending', NULL, ?, 11.5)",
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

test("legacy change histories keep their source sequence and separate same-ID hosts", async () => {
    const fixture = scopeFixture();
    try {
        await migrateLegacyBoard(fixture);
        const board = readMigratedBoard(fixture.destination, {
            accountId,
            userId,
            hosts: { local: "local", [sourceRemote]: installedRemote },
        });
        const local = board.changes[threadId];
        const remote = board.changes[installedId];
        assert.deepEqual(
            local?.map(({ changedAt, state, note, source }) => [
                source.sequence,
                changedAt,
                state,
                note,
            ]),
            [
                [1, 9, "review", "local history"],
                [2, 9, null, ""],
                [3, 8, "blocked", "local clock changed"],
            ],
        );
        assert.equal(remote?.[0]?.source.database, "status.sqlite3");
        assert.equal(remote?.[0]?.source.sequence, 4);
        assert.equal(remote?.[0]?.note, "cached remote history");
        const boundDatabase = `remote-hosts/${remoteDirectory(sourceRemote)}/status.sqlite3`;
        assert.deepEqual(
            remote?.slice(1).map(({ source, note }) => [source.database, source.sequence, note]),
            [
                [boundDatabase, 1, `${sourceRemote} history`],
                [boundDatabase, 2, ""],
                [boundDatabase, 3, `${sourceRemote} clock changed`],
            ],
        );
        assert.equal(board.changes[offlineId], undefined);
    } finally {
        fixture.close();
    }
});

function runtimeFixture(fixture: ReturnType<typeof scopeFixture>) {
    const directory = join(fixture.base, "runtime");
    const destination = join(directory, "legacy-migration");
    const bindingFile = join(directory, "legacy-binding.json");
    const recordsFile = join(directory, "records.json");
    const ledgersFile = join(directory, "ledger.json");
    mkdirSync(directory);
    const main = fixture.databases[0];
    assert.ok(main);
    main.prepare(
        "INSERT INTO archive_batch_items VALUES ('batch', 1, ?, ?, 'awaiting', NULL, NULL, NULL)",
    ).run(
        legacyRemoteId,
        JSON.stringify({
            id: legacyRemoteId,
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

test("migration rollback removes projected histories and keeps later manual changes", async () => {
    const fixture = scopeFixture();
    try {
        const { directory, destination, migration, records, recordsFile, writeBinding } =
            runtimeFixture(fixture);
        const imported = await migrateLegacyBoard({ ...fixture, destination });
        writeBinding({ local: "local", [sourceRemote]: installedRemote });
        const seed = migration.forAccount(storedAccount);
        assert.ok(seed?.board.changes[threadId]?.length);
        assert.ok(seed.board.changes[installedId]?.length);
        assert.equal(migration.forAccount(JSON.stringify([accountId, "another-user"])), undefined);
        const authored = [
            [threadId, "new local"],
            [installedId, "new remote"],
        ] as const;
        for (const [id, note] of authored) {
            records.write(
                {
                    account: storedAccount,
                    id,
                    expected: 12,
                    record: { note, manual: true, version: 13, inputVersion: 7 },
                    change: { kind: "manual", state: "completed" },
                },
                seed.board.cards,
            );
        }
        const saved = readFileSync(recordsFile, "utf8");
        const accounts = JSON.parse(saved) as Record<
            string,
            Record<string, { changes: { note: string }[] }>
        >;
        for (const [id, note] of authored) {
            const changes = accounts[storedAccount]?.[id]?.changes;
            assert.deepEqual(
                changes?.map(({ note }) => note),
                [note],
            );
        }
        rollbackLegacyMigration(destination, imported.receipt.id);
        assert.equal(new LegacyBoardRuntime(directory).forAccount(storedAccount), undefined);
        assert.equal(migration.forAccount(storedAccount), undefined);
        assert.equal(readFileSync(recordsFile, "utf8"), saved);
        const current = new BoardRecords(recordsFile).account(storedAccount);
        for (const [id, note] of authored) assert.equal(current[id]?.note, note);
    } finally {
        fixture.close();
    }
});

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
            ?.items.map(({ id, status, checkedAt }) => [id, status, checkedAt]),
        [
            [threadId, "pending", 11.5],
            [installedId, "awaiting", undefined],
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

function migrationThread(host: string): Thread {
    return {
        id: threadId as Thread["id"],
        name: `Native ${host}`,
        preview: "",
        cwd: "/native",
        projectId: null,
        updatedAt: 99,
        createdAt: 1,
        recencyAt: null,
        section: null,
        sectionEnteredAt: null,
        ephemeral: false,
        source: "vscode",
        status: { type: "notLoaded" },
        turns: [],
    };
}

function boardForMigration(runtime: ReturnType<typeof runtimeFixture>) {
    const context: SidebarContext = { accountId, userId, sections: [] };
    const hosts = ["local"];
    const nativeCalls: string[] = [];
    const available = new Set(["local"]);
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: hosts }),
        sidebar: async (command) => {
            assert.equal(command.kind, "context");
            return { ...context };
        },
        async send(event) {
            const { hostId, request } = event as {
                hostId: string;
                request: { id: string; method: string; params: { archived?: boolean } };
            };
            nativeCalls.push(hostId);
            if (!available.has(hostId)) throw new Error("fixture offline");
            const active = request.method === "thread/list" && !request.params.archived;
            assert.ok(
                ["thread/list", "thread/turns/list", "project/list"].includes(request.method),
            );
            desktop.receive({
                type: "mcp-response",
                hostId,
                message: {
                    id: request.id,
                    result: { data: active ? [migrationThread(hostId)] : [], nextCursor: null },
                },
            });
        },
    });
    const service = new BoardService(desktop, {
        load: async (key) => runtime.migration.loadRecords(key, runtime.records),
        loadLedger: async (key) =>
            runtime.ledgers.load(key, runtime.migration.forAccount(key)?.ledger),
        settings: async (key) => runtime.migration.settings(key, { syncTitles: false }),
        save: async () => assert.fail("Writing a cached card must be rejected"),
        preparePhase: async () => assert.fail("A cached card must not move"),
        settleTitle: async () => assert.fail("A cached card must not rename"),
        titleEpoch: async () => assert.fail("Title synchronization is disabled"),
        writeLedger: async () => assert.fail("A cached card cannot verify its archive receipt"),
        closePage: () => assert.fail("A cached card must not open"),
        authority: () => {},
        proof: () => {},
        updateStatus: async () => assert.fail("Unexpected update"),
    });
    return { context, hosts, available, nativeCalls, service };
}

async function boundMigration(test: TestContext) {
    const fixture = scopeFixture();
    const oldWindow = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    test.after(() => {
        Reflect.set(globalThis, "window", oldWindow);
        fixture.close();
    });
    const runtime = runtimeFixture(fixture);
    const imported = await migrateLegacyBoard({ ...fixture, destination: runtime.destination });
    runtime.writeBinding({ local: "local", [sourceOffline]: installedOffline });
    return { runtime, imported, ...boardForMigration(runtime) };
}

type BoardCard = ReturnType<BoardService["snapshot"]>["sessions"][number];

function assertOfflineCard(cached: BoardCard | undefined): asserts cached is BoardCard {
    assert.ok(cached);
    assert.partialDeepStrictEqual(cached, {
        hostId: installedOffline,
        threadId,
        title: "fixture",
        state: "to_archive",
        note: `${sourceOffline} WAL note`,
        version: 12,
        inputVersion: 7,
        followUp: true,
        titleSyncPending: true,
        titleSyncError: `${sourceOffline} title retry`,
        archivePending: true,
        catalogCurrent: false,
        readOnly: true,
        canOpen: false,
        canArchive: false,
        presence: "cachedUnknown",
        sectionVersion: null,
    });
    assert.equal(cached.previewTurnId, undefined);
}

async function assertReadOnly(service: BoardService, cached: BoardCard) {
    for (const action of ["/api/open", "/api/preview-reply", "/api/state", "/api/archive"]) {
        await assert.rejects(service.request(action, cached), /只显示缓存/);
    }
    await assert.rejects(
        service.request("/api/project/assign", {
            id: offlineId,
            projectId: "default",
            confirmed: true,
        }),
        /只显示缓存/,
    );
}

test("a bound remote migration is visible even if Codex has never connected", async (testContext) => {
    const { runtime, nativeCalls, service, hosts } = await boundMigration(testContext);
    await service.refresh();
    const first = service.snapshot();
    assert.deepEqual(new Set(nativeCalls), new Set(["local"]));
    assert.deepEqual(
        first.sessions.map(({ id }) => id),
        [threadId, offlineId],
    );
    assert.deepEqual(
        first.projectCatalog.remoteHosts.map(({ hostId }) => hostId),
        [installedOffline],
    );
    assert.equal(first.health.remoteConnections[0]?.connected, false);
    assert.equal(first.health.remoteConnections[0]?.scanComplete, false);
    const cached = first.sessions.find(({ id }) => id === offlineId);
    assertOfflineCard(cached);
    const priorCalls = nativeCalls.length;
    await assertReadOnly(service, cached);
    const { batches } = runtime.migration.forAccount(storedAccount)?.ledger ?? assert.fail();
    assert.deepEqual(await service.request("/api/archive-batches"), { batches });
    const remoteBatch = batches.find(({ items }) => items.some(({ id }) => id === offlineId));
    assert.ok(remoteBatch);
    assert.deepEqual(await service.request("/api/archive-batches/check", { id: remoteBatch.id }), {
        batch: remoteBatch,
    });
    assert.equal(nativeCalls.length, priorCalls);
    hosts.push(installedOffline);
    await service.refresh();
    assert.match(service.snapshot().health.remoteConnections[0]?.error ?? "", /fixture offline/);
    assert.deepEqual(
        service.snapshot().sessions.find(({ id }) => id === offlineId),
        cached,
    );
});

async function assertNoImportedCards(service: BoardService) {
    assert.deepEqual(
        service.snapshot().sessions.map(({ id, note }) => ({ id, note })),
        [{ id: threadId, note: "" }],
    );
    assert.deepEqual(await service.request("/api/archive-batches"), { batches: [] });
}

test("a migrated remote card belongs to the bound user and yields to native discovery", async (testContext) => {
    const { runtime, imported, context, hosts, available, service } =
        await boundMigration(testContext);
    await service.refresh();
    const first = service.snapshot();
    const cached = first.sessions.find(({ id }) => id === offlineId);
    assertOfflineCard(cached);

    context.userId = "other-user";
    await service.refresh();
    assert.notEqual(service.snapshot().accountScope, first.accountScope);
    await assertNoImportedCards(service);

    context.userId = userId;
    hosts.push(installedOffline);
    available.add(installedOffline);
    await service.refresh();
    const remote = service.snapshot().sessions.filter(({ id }) => id === offlineId);
    assert.equal(remote.length, 1);
    assert.partialDeepStrictEqual(remote[0], {
        title: `Native ${installedOffline}`,
        presence: "present",
        canOpen: true,
        readOnly: false,
        note: cached.note,
        version: cached.version,
    });
    assert.equal(
        runtime.migration.forAccount(storedAccount)?.board.changes[offlineId]?.[0]?.note,
        `${sourceOffline} history`,
    );

    rollbackLegacyMigration(runtime.destination, imported.receipt.id);
    const reopened = boardForMigration(runtime).service;
    await reopened.refresh();
    await assertNoImportedCards(reopened);
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
