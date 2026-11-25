import assert from "node:assert/strict";
import { test } from "node:test";
import { compactFragments } from "./format-tsx.mjs";

test("joins a single expression to the fragment tags and removes one indent level", () => {
    const expanded = `const view = (
    <>
        {items.map((item) => (
            <option
                key={item.value}
                value={item.value}
                title={item.path}
                data-host-id={item.hostId}>
                {item.label}
            </option>
        ))}
    </>
);
`;
    const compact = `const view = (
    <>{items.map((item) => (
        <option
            key={item.value}
            value={item.value}
            title={item.path}
            data-host-id={item.hostId}>
            {item.label}
        </option>
    ))}</>
);
`;
    assert.equal(compactFragments(expanded), compact);
    assert.equal(compactFragments(compact), compact);
});

test("preserves meaningful fragment text and multiple children", () => {
    const input = `const view = (
    <>
        text {label}
        {items}
    </>
);
const space = <> {label} </>;
const comment = <>{/* comment */}</>;
`;
    assert.equal(compactFragments(input), input);
});

test("compacts nested fragments without changing multiline literal content", () => {
    const expanded = `const view = (
    <>
        {items.map(() => (
            <>
                {String(
                    \`first
                    second\`,
                )}
            </>
        ))}
    </>
);
`;
    const compact = `const view = (
    <>{items.map(() => (
        <>{String(
            \`first
                    second\`,
        )}</>
    ))}</>
);
`;
    assert.equal(compactFragments(expanded), compact);
    assert.equal(compactFragments(compact), compact);
});
