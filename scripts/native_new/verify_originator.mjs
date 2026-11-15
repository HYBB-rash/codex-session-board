import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

const temporary = process.platform === "darwin" ? "/private/tmp" : os.tmpdir();
const root = fs.mkdtempSync(path.join(temporary, "csb-originator-"));
const executable = process.argv[2] || "codex";
const scenarios = [
  { name: "observer-first", clients: ["session_board_runtime", "codex_desktop"] },
  { name: "native-first", clients: ["codex_desktop", "session_board_runtime"] },
  { name: "native-environment", clients: ["session_board_runtime", "codex_desktop"],
    originator: "Codex Desktop" },
];

async function connect(address, name) {
  let socket;
  for (let attempt = 0; attempt < 60; attempt++) {
    try {
      socket = new WebSocket(address);
      await new Promise((resolve, reject) => {
        socket.addEventListener("open", resolve, { once: true });
        socket.addEventListener("error", reject, { once: true });
      });
      break;
    } catch { await new Promise(resolve => setTimeout(resolve, 50)); }
  }
  if (!socket || socket.readyState !== 1) throw Error("Isolated app-server did not start");
  let serial = 0;
  const pending = new Map();
  socket.addEventListener("message", event => {
    const response = JSON.parse(event.data), callback = pending.get(response.id);
    if (!callback) return;
    pending.delete(response.id);
    response.error ? callback.reject(Error(JSON.stringify(response.error))) : callback.resolve(response.result);
  });
  const call = (method, params) => new Promise((resolve, reject) => {
    const identifier = ++serial;
    const timer = setTimeout(() => {
      pending.delete(identifier); reject(Error(`${method} timed out`));
    }, 10000);
    pending.set(identifier, {
      resolve: value => { clearTimeout(timer); resolve(value); },
      reject: error => { clearTimeout(timer); reject(error); },
    });
    socket.send(JSON.stringify({ id: identifier, method, params }));
  });
  const initialized = await call("initialize", {
    clientInfo: { name, version: "originator-probe" }, capabilities: { experimentalApi: true },
  });
  socket.send(JSON.stringify({ method: "initialized" }));
  return { name, call, socket, userAgent: initialized.userAgent };
}

async function sample(scenario) {
  const directory = path.join(root, scenario.name);
  fs.mkdirSync(directory);
  const lease = net.createServer();
  await new Promise(resolve => lease.listen(0, "127.0.0.1", resolve));
  const address = `ws://127.0.0.1:${lease.address().port}`;
  await new Promise(resolve => lease.close(resolve));
  const environment = { ...process.env, CODEX_HOME: directory };
  for (const key of Object.keys(environment))
    if (/OPENAI_|CODEX_ACCESS_TOKEN|CODEX_INTERNAL_ORIGINATOR_OVERRIDE/.test(key)) delete environment[key];
  if (scenario.originator) environment.CODEX_INTERNAL_ORIGINATOR_OVERRIDE = scenario.originator;
  const server = spawn(executable, ["app-server", "--listen", address], {
    cwd: directory, env: environment, stdio: ["ignore", "ignore", "ignore"],
  });
  const clients = [];
  try {
    for (const name of scenario.clients) clients.push(await connect(address, name));
    const native = clients.find(client => client.name === "codex_desktop");
    const result = await native.call("thread/start", {
      cwd: directory, ephemeral: true, model: "gpt-5.1-codex",
    });
    return { scenario: scenario.name, originator: result.thread.originator,
      userAgents: clients.map(client => client.userAgent) };
  } finally {
    for (const client of clients) client.socket.close();
    server.kill("SIGTERM");
    await new Promise(resolve => {
      server.once("exit", resolve);
      setTimeout(() => { server.kill("SIGKILL"); resolve(); }, 2000).unref();
    });
  }
}

try {
  const results = [];
  for (const scenario of scenarios) results.push(await sample(scenario));
  console.log(JSON.stringify({ protocolOnly: true, modelTurnsStarted: 0, results }, null, 2));
} finally { fs.rmSync(root, { recursive: true, force: true }); }
