import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
import { findCalledJsxRenderers, findForwardedRenders } from "./check-tsx-render.mjs";

const component = 'import { defineComponent as component } from "vue";';
const helper = "function BuildArticle() { return <article />; }";
const forwarded = "component({ setup() { return () => BuildArticle(); } });";

function check(source, rule = findForwardedRenders, extra = {}) {
    const filename = ts.sys.resolvePath("case.tsx");
    const files = new Map(
        Object.entries({ "case.tsx": source, ...extra }).map(([name, text]) => [
            ts.sys.resolvePath(name),
            text,
        ]),
    );
    const options = {
        jsx: ts.JsxEmit.Preserve,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
        noLib: true,
        types: [],
    };
    const host = ts.createCompilerHost(options);
    host.fileExists = (name) => files.has(name);
    host.getSourceFile = (name, version) => {
        const text = files.get(name);
        return text === undefined ? undefined : ts.createSourceFile(name, text, version, true);
    };
    const program = ts.createProgram([filename], options, host);
    const file = program.getSourceFile(filename);
    return rule(file, program.getTypeChecker()).map((call) => call.expression.getText(file));
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

test("rejects direct render calls through local, imported, and object bindings", () => {
    assert.deepEqual(
        check(
            `
                import { RenderHeader as Header } from "./header";
                const body = { render: () => true ? <p /> : null };
                function Footer() { return <footer />; }
                const footer = Footer();
                const page = <main>{Header()}{body.render()}{footer}</main>;
            `,
            findCalledJsxRenderers,
            { "header.tsx": "export function RenderHeader() { return <header />; }" },
        ),
        ["Footer", "Header", "body.render"],
    );
});

test("rejects calls to helpers that build lists of JSX nodes", () => {
    assert.deepEqual(
        check(
            `
                const options = (values: string[]) => values.map((value) => <option>{value}</option>);
                const fixed = () => [<option />, <option />];
                const page = <select>{options(["first"])}{fixed()}</select>;
            `,
            findCalledJsxRenderers,
        ),
        ["options", "fixed"],
    );
});

test("allows JSX tags, inline mapping, text helpers, and shadowed non-rendering functions", () => {
    assert.deepEqual(
        check(
            `
                ${helper}
                function label(value: string) { return value.toUpperCase(); }
                function other(BuildArticle: () => string) { return BuildArticle(); }
                const page = <main>
                    <BuildArticle />
                    {["one"].map((value) => <p>{label(value)}</p>)}
                </main>;
            `,
            findCalledJsxRenderers,
        ),
        [],
    );
});
