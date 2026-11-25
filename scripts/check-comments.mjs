import { readdirSync, readFileSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { parseSync } from "oxc-parser";
import { parse } from "vue/compiler-sfc";

const root = join(import.meta.dirname, "..");
const referenceDirective = /^\/\/\/\s*<reference\b[^>]*>\s*$/;

function sourceFiles(directory, found = []) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) sourceFiles(path, found);
        else if (/\.(?:[cm]?tsx?|[cm]?jsx?|vue)$/.test(entry.name)) found.push(path);
    }
    return found;
}

function language(filename) {
    const extension = extname(filename);
    if (extension === ".tsx") return "tsx";
    if (extension === ".jsx") return "jsx";
    return extension.endsWith("ts") ? "ts" : "js";
}

function lineOf(source, offset) {
    let line = 1;
    for (let index = 0; index < offset; index += 1) {
        if (source.charCodeAt(index) === 10) line += 1;
    }
    return line;
}

function checkScript(file, source, lang, firstLine = 1) {
    const parsed = parseSync(file, source, { lang, sourceType: "module" });
    if (parsed.errors.length > 0) {
        violations.push(`${relative(root, file)}: 无法解析`);
        return;
    }
    for (const comment of parsed.comments) {
        const text = source.slice(comment.start, comment.end);
        if (referenceDirective.test(text)) continue;
        const line = firstLine + lineOf(source, comment.start) - 1;
        violations.push(`${relative(root, file)}:${line}: 不允许写注释`);
    }
}

const violations = [];
for (const file of sourceFiles(join(root, "src"))) {
    const source = readFileSync(file, "utf8");
    if (!file.endsWith(".vue")) {
        checkScript(file, source, language(file));
        continue;
    }
    const { descriptor, errors } = parse(source, { filename: file });
    if (errors.length > 0) {
        violations.push(`${relative(root, file)}: 无法解析`);
        continue;
    }
    for (const block of [descriptor.script, descriptor.scriptSetup]) {
        if (block) checkScript(file, block.content, block.lang ?? "js", block.loc.start.line);
    }
}

if (violations.length > 0) {
    for (const violation of violations) console.error(violation);
    process.exit(1);
}
