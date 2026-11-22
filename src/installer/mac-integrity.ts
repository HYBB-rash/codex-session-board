import * as fs from "node:fs";
import * as path from "node:path";

export const sentinel = Buffer.from("AGbevlPCksUGKNL8TSn7wGmJEuJsXb2A");
const slotLength = sentinel.length + 2 + 32;

export function hasEmbeddedIntegrityDigest(binary: string): boolean {
    const file = fs.openSync(binary, "r");
    const chunkSize = 1024 * 1024;
    const chunk = Buffer.alloc(chunkSize);
    try {
        const size = fs.fstatSync(file).size;
        for (let offset = 0; offset < size; offset += chunkSize - slotLength + 1) {
            const length = fs.readSync(file, chunk, 0, chunkSize, offset);
            const candidate = chunk.subarray(0, length);
            let index = candidate.indexOf(sentinel);
            while (index >= 0 && index + slotLength <= length) {
                if (candidate[index + sentinel.length] === 1) return true;
                index = candidate.indexOf(sentinel, index + sentinel.length);
            }
        }
        return false;
    } finally {
        fs.closeSync(file);
    }
}

export function assertPatchableMacBundle(appDirectory: string, version: string): void {
    const frameworks = path.join(appDirectory, "Contents", "Frameworks");
    for (const entry of fs.readdirSync(frameworks, { withFileTypes: true })) {
        if (!entry.name.endsWith(".framework")) continue;
        const name = path.basename(entry.name, ".framework");
        const binary = path.join(frameworks, entry.name, name);
        if (!fs.statSync(binary, { throwIfNoEntry: false })?.isFile()) continue;
        if (hasEmbeddedIntegrityDigest(binary)) {
            throw new Error(
                `Codex ${version} embeds the app.asar integrity dictionary digest in ${name}; the startup-entry patch cannot preserve it`,
            );
        }
    }
}
