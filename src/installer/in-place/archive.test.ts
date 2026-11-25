import assert from "node:assert/strict";
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import test, { type TestContext } from "node:test";
import { createArchive, entryIncludes, sha256File } from "../asar.js";
import { SENTINEL } from "../constants.js";
import {
    archivePosition,
    commitArchive,
    prepareArchive,
    restoreArchive,
    writeInstallationReceipt,
} from "./archive.js";
import { fileProof } from "./target.js";

function fixture(context: TestContext) {
    const directory = fs.mkdtempSync(path.join(tmpdir(), "enhancer-in-place-"));
    context.after(() => fs.rmSync(directory, { recursive: true, force: true }));
    const transaction = {
        archive: path.join(directory, "app.asar"),
        backup: path.join(directory, "archive.backup"),
        staged: path.join(directory, ".staged"),
        receipt: path.join(directory, "installation.json"),
    };
    const entry = "main.js";
    createArchive(transaction.archive, [
        { path: "package.json", data: Buffer.from('{"main":"main.js"}') },
        { path: entry, data: Buffer.from("process.exit(0);\n") },
    ]);
    fs.chmodSync(transaction.archive, 0o640);
    fs.utimesSync(transaction.archive, 1000000, 1000000);
    const before = fileProof(transaction.archive);
    const proofs = prepareArchive(transaction, entry, before, `/*${SENTINEL}*/\n`);
    return { transaction, before, proofs, entry };
}

const linuxOnly = { skip: process.platform !== "linux" };

test(
    "an in-place archive can resume a prepared install and restore its original metadata",
    linuxOnly,
    (context) => {
        const { transaction, before, proofs, entry } = fixture(context);
        const stopped = context.mock.fn(() => {});
        writeInstallationReceipt(transaction.receipt, { ...proofs, phase: "prepared" });
        assert.equal(archivePosition(transaction, proofs), "original");
        assert.deepEqual(fileProof(transaction.backup), before);
        assert.equal(JSON.parse(fs.readFileSync(transaction.receipt, "utf8")).phase, "prepared");

        commitArchive(transaction, proofs, stopped);
        assert.equal(archivePosition(transaction, proofs), "patched");
        assert.equal(entryIncludes(transaction.archive, entry, Buffer.from(SENTINEL)), true);
        commitArchive(transaction, proofs, stopped);
        assert.equal(stopped.mock.callCount(), 1);

        restoreArchive(transaction, proofs, stopped);
        assert.deepEqual(fileProof(transaction.archive), before);
        assert.equal(archivePosition(transaction, proofs), "original");
        assert.equal(stopped.mock.callCount(), 2);
    },
);

test(
    "an in-place archive is unchanged if a write fails or the RPM replaces the target",
    linuxOnly,
    (context) => {
        const { transaction, before, proofs, entry } = fixture(context);
        const stopped = () => {};
        const rename = fs.renameSync;
        const failRename: typeof fs.renameSync = (first, second) => {
            if (second === transaction.archive) throw new Error("simulated write failure");
            rename(first, second);
        };
        const failure = context.mock.method(
            require("node:fs") as typeof fs,
            "renameSync",
            failRename,
        );
        assert.throws(() => commitArchive(transaction, proofs, stopped), /simulated write failure/);
        assert.deepEqual(fileProof(transaction.archive), before);
        failure.mock.restore();
        commitArchive(transaction, proofs, stopped);

        createArchive(transaction.archive, [
            { path: "package.json", data: Buffer.from('{"main":"main.js"}') },
            { path: entry, data: Buffer.from("process.exit(1);\n") },
        ]);
        const replaced = sha256File(transaction.archive);
        assert.equal(archivePosition(transaction, proofs), "unknown");
        assert.throws(() => restoreArchive(transaction, proofs, stopped), /归档已变化/);
        assert.equal(sha256File(transaction.archive), replaced);
        assert.deepEqual(fileProof(transaction.backup), before);
    },
);
