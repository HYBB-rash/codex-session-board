import { existsSync, readFileSync } from "node:fs";
import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";
import { build } from "esbuild";
import { defineConfig, type Plugin, build as viteBuild } from "vite";

const packageInfo = JSON.parse(readFileSync("package.json", "utf8")) as { version: string };
const version = JSON.stringify(packageInfo.version);
function standaloneAssets(): Plugin {
    return {
        name: "session-board-assets",
        async generateBundle() {
            const app = await viteBuild({
                ...settings("mcp", "standalone-mcp.ts", false),
                configFile: false,
            });
            const outputs = Array.isArray(app) ? app : [app];
            const code = outputs
                .flatMap((item) => ("output" in item ? item.output : []))
                .find((entry) => entry.type === "chunk")?.code;
            if (!code) throw new Error("无法生成 MCP App");
            const script = code.replace(/<\/script/gi, "<\\/script");
            const html = readFileSync("src/ui/standalone-index.html", "utf8");
            this.emitFile({ type: "asset", fileName: "index.html", source: html });
            this.emitFile({
                type: "asset",
                fileName: "version.txt",
                source: `${packageInfo.version}\n`,
            });
            this.emitFile({
                type: "asset",
                fileName: "mcp.html",
                source: `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>会话看板</title></head><body><script>${script}</script></body></html>`,
            });
            for (const fileName of ["board.svg", "manifest.webmanifest"]) {
                this.emitFile({
                    type: "asset",
                    fileName,
                    source: readFileSync(`src/ui/assets/${fileName}`),
                });
            }
            this.emitFile({
                type: "asset",
                fileName: "board-operations.json",
                source: readFileSync("src/ui/shared/board-operations.json"),
            });
            const worker = await build({
                entryPoints: ["src/ui/standalone-sw.ts"],
                bundle: true,
                write: false,
                minify: true,
                target: "es2022",
            });
            const sw = worker.outputFiles[0];
            if (!sw) throw new Error("无法生成 PWA 服务组件");
            this.emitFile({ type: "asset", fileName: "sw.js", source: sw.contents });
        },
    };
}

function settings(name: string, entry: string, write: boolean) {
    return defineConfig({
        publicDir: false,
        logLevel: "error",
        define: {
            __VERSION__: version,
            __VUE_OPTIONS_API__: false,
            "process.env.NODE_ENV": JSON.stringify("production"),
        },
        plugins: [
            vue({
                script: {
                    fs: {
                        fileExists: existsSync,
                        readFile: (path) =>
                            existsSync(path) ? readFileSync(path, "utf8") : undefined,
                    },
                },
            }),
            vueJsx(),
            ...(write ? [standaloneAssets()] : []),
        ],
        build: {
            write,
            target: "es2022",
            outDir: "dist/board-assets",
            emptyOutDir: true,
            lib: {
                name: "SessionBoard",
                entry: `src/ui/${entry}`,
                formats: ["iife"],
                fileName: () => `${name}.js`,
            },
        },
    });
}

export default settings("board", "standalone-web.ts", true);
