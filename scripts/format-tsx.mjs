import { execFileSync } from "node:child_process";
import { globSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const lineBreak = /\r?\n/;
const indentation = " ".repeat(
    JSON.parse(readFileSync(new URL("../biome.json", import.meta.url), "utf8")).formatter
        .indentWidth,
);

function isLayoutText(node) {
    return ts.isJsxText(node) && node.containsOnlyTriviaWhiteSpaces && lineBreak.test(node.text);
}

function collectFragmentEdits(fragment, source, removals, dedents) {
    const children = fragment.children.filter((child) => !isLayoutText(child));
    const [child] = children;
    if (children.length !== 1 || !ts.isJsxExpression(child) || !child.expression) return;

    const openingEnd = fragment.openingFragment.end;
    const childStart = child.getStart(source);
    const closingStart = fragment.closingFragment.getStart(source);
    if (lineBreak.test(source.text.slice(openingEnd, childStart))) {
        removals.push([openingEnd, childStart]);
        const firstLine = source.getLineAndCharacterOfPosition(childStart).line;
        const lastLine = source.getLineAndCharacterOfPosition(child.end).line;
        for (let line = firstLine + 1; line <= lastLine; line++) {
            dedents.set(line, (dedents.get(line) ?? 0) + 1);
        }
    }
    if (lineBreak.test(source.text.slice(child.end, closingStart))) {
        removals.push([child.end, closingStart]);
    }
}

function protectedLines(node, source, lines) {
    const isLiteral = ts.isStringLiteralLike(node) || ts.isTemplateLiteralToken(node);
    const isText = ts.isJsxText(node) && !node.containsOnlyTriviaWhiteSpaces;
    if (!isLiteral && !isText) return;

    const start = source.getLineAndCharacterOfPosition(isText ? node.pos : node.getStart(source));
    const end = source.getLineAndCharacterOfPosition(node.end);
    for (let line = start.line + 1; line <= end.line; line++) lines.add(line);
}

export function compactFragments(text) {
    const source = ts.createSourceFile("source.tsx", text, ts.ScriptTarget.Latest);
    const removals = [];
    const dedents = new Map();
    const literalLines = new Set();
    function visit(node) {
        if (ts.isJsxFragment(node)) collectFragmentEdits(node, source, removals, dedents);
        protectedLines(node, source, literalLines);
        ts.forEachChild(node, visit);
    }
    visit(source);
    const lineStarts = source.getLineStarts();
    for (const [line, count] of dedents) {
        if (literalLines.has(line)) continue;
        const start = lineStarts[line];
        let end = start;
        while (end - start < count * indentation.length && text.startsWith(indentation, end)) {
            end += indentation.length;
        }
        if (end > start) removals.push([start, end]);
    }

    let result = "";
    let cursor = 0;
    for (const [start, end] of removals.sort(([left], [right]) => left - right)) {
        if (start > cursor) result += text.slice(cursor, start);
        cursor = Math.max(cursor, end);
    }
    return result + text.slice(cursor);
}

function run(write) {
    for (const path of globSync("src/**/*.tsx").sort()) {
        const input = readFileSync(path, "utf8");
        const biomeOutput = execFileSync(
            "biome",
            [
                "format",
                "--config-path",
                "scripts/biome-tsx.json",
                "--write",
                "--stdin-file-path",
                path,
            ],
            { input, encoding: "utf8" },
        );
        const output = compactFragments(biomeOutput);
        if (input === output) continue;
        if (write) writeFileSync(path, output);
        else {
            console.error(`${path} needs formatting. Run npm run format.`);
            process.exitCode = 1;
        }
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    run(process.argv.includes("--write"));
}
