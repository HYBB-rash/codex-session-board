import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { Agent, createServer, request } from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { boardScript, loadBoardVite } from "./build-reply-preview.mjs";

function sendError(response, status, error) {
  response.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store" });
  response.end(JSON.stringify({ error }));
}

async function withFixture(incoming, response, target, record) {
  const agent = new Agent({ keepAlive: true, maxSockets: 1 });
  const connect = agent.createConnection.bind(agent);
  agent.createConnection = (options, callback) => {
    agent.createConnection = (_, reject) => reject(new Error("隔离 fixture 的身份连接已关闭"));
    return connect(options, callback);
  };
  const forward = (url, options, body) => new Promise((resolve, reject) => {
    const upstream = request(url, { ...options, agent }, resolve);
    upstream.on("error", reject);
    upstream.setTimeout(30000, () => upstream.destroy(new Error("隔离 fixture 请求超时")));
    if (body) body.pipe(upstream);
    else upstream.end();
  });
  response.once("close", () => agent.destroy());
  try {
    const info = await forward(new URL("/api/info", target));
    const chunks = [];
    for await (const chunk of info) chunks.push(chunk);
    const content = Buffer.concat(chunks);
    const identity = JSON.parse(content);
    if (info.statusCode !== 200 || identity.pid !== record.service_pid ||
        identity.build !== record.program_sha256 || identity.dataDir !== path.join(record.root, "data"))
      throw new Error("隔离 fixture 身份不匹配");
    const headers = { "Cache-Control": "no-store", "Content-Type": "application/json" };
    if (incoming.url === "/api/info") {
      response.writeHead(200, headers).end(content);
      return;
    }
    const writing = incoming.method === "POST";
    const upstream = await forward(new URL(incoming.url, target), {
      method: incoming.method,
      headers: writing ? {
        Origin: target.origin, "Content-Type": "application/json",
        "Content-Length": incoming.headers["content-length"],
        "X-CSRF-Token": incoming.headers["x-csrf-token"],
      } : {},
    }, writing ? incoming : undefined);
    response.writeHead(upstream.statusCode, headers);
    upstream.on("error", error => response.destroy(error)).pipe(response);
  } catch {
    if (!response.destroyed) sendError(response, 502, "隔离开发会话不可用，请检查 scripts/dev status");
  }
}

export async function serveFrontend(root, temporary) {
  const record = JSON.parse(execFileSync(path.join(root, "scripts/dev"), ["status"], { encoding: "utf8" }));
  if (record.phase !== "running" || record.source !== root || !/^http:\/\/127\.0\.0\.1:[1-9]\d*$/.test(record.url))
    throw new Error("请先在本工作树运行 ./scripts/dev start");
  const target = new URL(record.url);
  const operations = JSON.parse(await readFile(path.join(root, "assets/board-operations.json"), "utf8"));
  const blocked = new Set(["Connect", "SshSetup", "PluginUpdateStart"]);
  const allowed = new Set(["GET /api/info", ...operations.filter(operation => !blocked.has(operation.action))
    .map(operation => `${operation.method} ${operation.path}`)]);
  const { vite, config } = await loadBoardVite(root, temporary);
  const require = createRequire(path.join(temporary, "package.json"));
  const http = createServer();
  const server = await vite.createServer({
    ...config, root, cacheDir: path.join(temporary, ".vite"),
    resolve: { alias: [{ find: /^vue$/, replacement: require.resolve("vue/dist/vue.runtime.esm-bundler.js") }] },
    optimizeDeps: { noDiscovery: true },
    server: { host: "127.0.0.1", cors: false, middlewareMode: true, hmr: { server: http },
      fs: { allow: [path.join(root, "index.html"), path.join(root, "assets"), temporary] } },
    plugins: [...config.plugins, {
      name: "board-dev-fixture",
      transformIndexHtml: { order: "pre", handler(html) {
        return html.replace(boardScript, "").replace(/<script>\n([\s\S]*?)<\/script>/, (_, legacy) =>
          `<script type="module">\nimport "/assets/board-start.ts";\n${legacy}</script>`);
      } },
      configureServer(server) {
        server.middlewares.use((incoming, response, next) => {
          const host = `127.0.0.1:${http.address()?.port}`;
          const origin = incoming.headers.origin;
          const fetchSite = incoming.headers["sec-fetch-site"];
          if (incoming.headers.host !== host || (origin && origin !== `http://${host}`) ||
              (fetchSite && fetchSite !== "same-origin" && fetchSite !== "none"))
            return sendError(response, 403, "请从隔离开发页面操作");
          if (incoming.url.startsWith("/__open-in-editor"))
            return sendError(response, 404, "请在工作树编辑源码");
          if (!incoming.url.startsWith("/api/")) return next();
          if (!allowed.has(`${incoming.method} ${incoming.url}`))
            return sendError(response, 404, "隔离开发环境未开放此接口");
          if (incoming.method === "POST" &&
              (origin !== `http://${host}` || !incoming.headers["x-csrf-token"] || !incoming.headers["content-length"]))
            return sendError(response, 403, "开发请求缺少来源或写入校验");
          void withFixture(incoming, response, target, record);
        });
      },
    }],
  });
  try {
    http.on("request", server.middlewares);
    await new Promise((resolve, reject) => {
      http.once("error", reject);
      http.listen(0, "127.0.0.1", resolve);
    });
    console.log(`Vite: http://127.0.0.1:${http.address().port}/`);
    await new Promise(resolve => {
      process.once("SIGINT", resolve);
      process.once("SIGTERM", resolve);
    });
  } finally {
    const closed = new Promise(resolve => http.close(resolve));
    http.closeAllConnections();
    await server.close();
    await closed;
  }
}
