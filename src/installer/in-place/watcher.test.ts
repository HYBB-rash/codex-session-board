import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import {
    checkInPlaceUpgrade,
    type InPlaceWatchStatus,
    systemWatcherFiles,
    systemWatcherLinks,
} from "./watcher.js";

test("enabling the user watcher monitors the archive directory and probes on login", () => {
    const units = systemWatcherFiles({
        node: "/opt/node with spaces/bin/node",
        cli: '/opt/codex %version%/dist/"quoted".js',
        watch: "/opt/codex %version%/resources",
    });
    const service = "/etc/systemd/user/codex-enhancer-upgrade-check.service";
    const watcher = "/etc/systemd/user/codex-enhancer-upgrade-check.path";
    assert.deepEqual([...units.keys()], [service, watcher]);
    assert.ok(units.get(service)?.includes("Type=oneshot\n"));
    assert.ok(
        units
            .get(service)
            ?.includes(
                'ExecStart=:"/opt/node with spaces/bin/node" "/opt/codex %%version%%/dist/\\"quoted\\".js" in-place-check\n',
            ),
    );
    assert.ok(units.get(watcher)?.includes("PathChanged=/opt/codex %%version%%/resources\n"));
    assert.ok(units.get(watcher)?.includes("Unit=codex-enhancer-upgrade-check.service\n"));
    assert.deepEqual(
        [...systemWatcherLinks()].map(([link, target]) => [
            dirname(link),
            resolve(dirname(link), target),
        ]),
        [
            ["/etc/systemd/user/default.target.wants", service],
            ["/etc/systemd/user/graphical-session.target.wants", service],
            ["/etc/systemd/user/default.target.wants", watcher],
        ],
    );
    assert.match(units.get(service) ?? "", /WantedBy=default\.target graphical-session\.target\n/);
    assert.match(units.get(watcher) ?? "", /WantedBy=default\.target\n/);
});

test("unit generation rejects relative paths and line continuations", () => {
    const commands = {
        node: "/usr/bin/node",
        cli: "/opt/enhancer/cli.js",
        watch: "/opt/codex/resources",
    };
    assert.throws(() => systemWatcherFiles({ ...commands, node: "node" }), /absolute/);
    assert.throws(
        () => systemWatcherFiles({ ...commands, cli: "/opt/enhancer\n/cli.js" }),
        /single-line/,
    );
    assert.throws(() => systemWatcherFiles({ ...commands, watch: "/opt/codex\\" }), /continuation/);
});

test("a new login reports an existing upgrade once until the observed problem changes", async (context) => {
    const root = mkdtempSync(join(tmpdir(), "enhancer-upgrade-watch-"));
    context.after(() => rmSync(root, { recursive: true, force: true }));
    const privateDirectory = join(root, "user-state");
    const receipt = join(privateDirectory, "upgrade-watch.json");
    const notices: string[] = [];
    let status: InPlaceWatchStatus = { kind: "ready" };
    const check = () =>
        checkInPlaceUpgrade(
            privateDirectory,
            () => status,
            ({ message }) => {
                notices.push(message);
            },
        );

    assert.equal((await check()).notified, false);
    assert.equal(existsSync(privateDirectory), false);
    status = {
        kind: "loader-replaced",
        fingerprint: "rpm-build-a",
        message: "Codex was reinstalled.",
    };
    assert.equal((await check()).notified, true);
    assert.deepEqual(JSON.parse(readFileSync(receipt, "utf8")), {
        kind: status.kind,
        fingerprint: status.fingerprint,
    });
    assert.equal(statSync(receipt).mode & 0o777, 0o600);
    assert.equal(statSync(privateDirectory).mode & 0o777, 0o700);
    status = { ...status, message: "Only the diagnostic wording changed." };
    assert.equal((await check()).notified, false);
    status = { ...status, fingerprint: "rpm-build-b", message: "Codex was updated." };
    assert.equal((await check()).notified, true);
    status = { ...status, kind: "incompatible", message: "This build is incompatible." };
    assert.equal((await check()).notified, true);
    status = { ...status, kind: "unverifiable", message: "Installation could not be verified." };
    assert.equal((await check()).notified, true);
    assert.deepEqual(notices, [
        "Codex was reinstalled.",
        "Codex was updated.",
        "This build is incompatible.",
        "Installation could not be verified.",
    ]);
});

test("each private directory records its own notice and can start fresh after uninstall", async (context) => {
    const root = mkdtempSync(join(tmpdir(), "enhancer-upgrade-watch-"));
    context.after(() => rmSync(root, { recursive: true, force: true }));
    const firstUser = join(root, "first-user");
    const secondUser = join(root, "second-user");
    let status: InPlaceWatchStatus = {
        kind: "loader-replaced",
        fingerprint: "rpm-build-a",
        message: "Codex was reinstalled.",
    };
    let notices = 0;
    const check = (directory: string) =>
        checkInPlaceUpgrade(
            directory,
            () => status,
            () => {
                notices++;
            },
        );

    const problem = status;
    assert.equal((await check(firstUser)).notified, true);
    assert.equal((await check(secondUser)).notified, true);
    status = { kind: "not-installed" };
    assert.equal((await check(firstUser)).notified, false);
    assert.equal(existsSync(join(firstUser, "upgrade-watch.json")), false);
    status = problem;
    assert.equal((await check(firstUser)).notified, true);
    assert.equal((await check(secondUser)).notified, false);
    assert.equal(notices, 3);
});

test("a failed desktop notification is left eligible for the next check", async (context) => {
    const root = mkdtempSync(join(tmpdir(), "enhancer-upgrade-watch-"));
    context.after(() => rmSync(root, { recursive: true, force: true }));
    const status = {
        kind: "loader-replaced",
        fingerprint: "rpm-build-a",
        message: "Use a compatible release.",
    } as const;
    await assert.rejects(
        checkInPlaceUpgrade(
            root,
            async () => status,
            async () => {
                throw new Error("desktop notifications unavailable");
            },
        ),
        /desktop notifications unavailable/,
    );
    assert.equal(existsSync(join(root, "upgrade-watch.json")), false);
    assert.equal(
        (
            await checkInPlaceUpgrade(
                root,
                () => status,
                () => {},
            )
        ).notified,
        true,
    );
});
