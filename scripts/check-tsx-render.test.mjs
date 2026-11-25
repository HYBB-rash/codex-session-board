import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
import { findForwardedRenders } from "./check-tsx-render.mjs";

const component = 'import { defineComponent as component } from "vue";';
const helper = "function BuildArticle() { return <article />; }";
const forwarded = "component({ setup() { return () => BuildArticle(); } });";

function check(source) {
    const filename = "case.tsx";
    const file = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    const options = { jsx: ts.JsxEmit.Preserve, noLib: true, noResolve: true, types: [] };
    const host = ts.createCompilerHost(options);
    host.getSourceFile = (name) => (name === filename ? file : undefined);
    const program = ts.createProgram([filename], options, host);
    return findForwardedRenders(file, program.getTypeChecker()).map((call) => call.expression.text);
}

test("finds a renamed helper through its binding, including with shadowed names", () => {
    assert.deepEqual(
        check(
            `${component} ${helper} function other(BuildArticle) { BuildArticle(); } ${forwarded}`,
        ),
        ["BuildArticle"],
    );
});

test("finds a block-bodied forwarding render created by createApp", () => {
    assert.deepEqual(
        check(`
            import { createApp } from "vue";
            const build = () => <section />;
            createApp({ setup: () => () => { return build(); } });
        `),
        ["build"],
    );
});

test("allows inline JSX and helpers used as JSX subcomponents", () => {
    assert.deepEqual(
        check(
            `${component} ${helper} component({ setup: () => () => <main><BuildArticle /></main> });`,
        ),
        [],
    );
});

test("allows helpers shared through JSX, a shorthand property, or an export", () => {
    for (const use of [
        "const node = <BuildArticle />;",
        "const callbacks = { BuildArticle };",
        "export { BuildArticle as PublicComponent };",
        "export default BuildArticle;",
    ]) {
        assert.deepEqual(check(`${component} ${helper} ${forwarded} ${use}`), [], use);
    }
});

test("ignores non-Vue setup methods and calls to helpers that return data", () => {
    assert.deepEqual(check(`${helper} const component = (options) => options; ${forwarded}`), []);
    assert.deepEqual(
        check(`${component} function BuildArticle() { return "data"; } ${forwarded}`),
        [],
    );
});
