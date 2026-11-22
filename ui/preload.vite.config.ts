import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";
import { defineConfig } from "vite";

export default defineConfig({
    plugins: [
        vue({
            script: {
                fs: {
                    fileExists: existsSync,
                    readFile: (path) => existsSync(path) ? readFileSync(path, "utf8") : undefined,
                },
            },
        }),
        vueJsx(),
    ],
    build: {
        target: "es2022",
        outDir: "dist/runtime",
        emptyOutDir: false,
        lib: {
            entry: "ui/preload-entry.ts",
            formats: ["cjs"],
            fileName: () => "preload.js",
        },
        rollupOptions: { external: ["electron"] },
    },
});
import { existsSync, readFileSync } from "node:fs";
