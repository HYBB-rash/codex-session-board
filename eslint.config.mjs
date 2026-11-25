import vue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";

const interpolationSpacing = {
    meta: {
        fixable: "whitespace",
        messages: {
            space: "Use one space between the interpolation delimiter and its expression.",
        },
    },
    create(context) {
        const { sourceCode } = context;
        const { defineTemplateBodyVisitor, getTemplateBodyTokenStore } = sourceCode.parserServices;
        const tokens = getTemplateBodyTokenStore();
        const includeComments = { includeComments: true };

        function checkGap(node, start, end) {
            if (sourceCode.text.slice(start, end) === " ") return;
            context.report({
                node,
                messageId: "space",
                fix: (fixer) => fixer.replaceTextRange([start, end], " "),
            });
        }

        return defineTemplateBodyVisitor({
            "VExpressionContainer[expression!=null]"(node) {
                const opening = tokens.getFirstToken(node);
                const closing = tokens.getLastToken(node);
                if (opening?.type !== "VExpressionStart" || closing?.type !== "VExpressionEnd") {
                    return;
                }
                const first = tokens.getTokenAfter(opening, includeComments);
                const last = tokens.getTokenBefore(closing, includeComments);
                checkGap(opening, opening.range[1], first.range[0]);
                if (last.type !== "Line") checkGap(closing, last.range[1], closing.range[0]);
            },
        });
    },
};

export default [
    {
        files: ["**/*.vue"],
        languageOptions: {
            parser: vueParser,
            parserOptions: {
                parser: false,
            },
        },
        plugins: { vue, local: { rules: { "interpolation-spacing": interpolationSpacing } } },
        rules: {
            "vue/html-closing-bracket-newline": ["error", { multiline: "never" }],
            "vue/html-indent": ["error", 4, { baseIndent: 0 }],
            "local/interpolation-spacing": "error",
        },
    },
];
