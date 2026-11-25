import vue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";

export default [
    {
        files: ["**/*.vue"],
        languageOptions: {
            parser: vueParser,
            parserOptions: {
                parser: false,
            },
        },
        plugins: { vue },
        rules: {
            "vue/html-closing-bracket-newline": ["error", { multiline: "never" }],
            "vue/html-indent": ["error", 4, { baseIndent: 0 }],
            "vue/mustache-interpolation-spacing": ["error", "never"],
        },
    },
];
