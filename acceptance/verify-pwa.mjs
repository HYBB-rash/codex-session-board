const port = Number(process.argv[2]);
const base = process.argv[3] ?? "http://127.0.0.1:18767/";
if (!Number.isSafeInteger(port) || port <= 0) throw new Error("需要独立 Chrome 配置的调试端口");
const endpoint = `http://127.0.0.1:${port}`;
const targets = () => fetch(`${endpoint}/json/list`).then((response) => response.json());

async function connect(url) {
    const socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
        socket.onopen = resolve;
        socket.onerror = reject;
    });
    let sequence = 0;
    const replies = new Map();
    socket.onmessage = ({ data }) => {
        const response = JSON.parse(data);
        replies.get(response.id)?.(response);
        replies.delete(response.id);
    };
    return {
        async call(method, params = {}) {
            const id = ++sequence;
            const value = await new Promise((resolve) => {
                replies.set(id, resolve);
                socket.send(JSON.stringify({ id, method, params }));
            });
            if (value.error) throw new Error(`${method} ${value.error.message}`);
            return value.result;
        },
        close: () => socket.close(),
    };
}

async function inspectWindow(client) {
    const response = await client.call("Runtime.evaluate", {
        expression: `({
            board: Boolean(document.getElementById('board-viewport')),
            worker: Boolean(navigator.serviceWorker.controller),
            standalone: matchMedia('(display-mode: standalone)').matches,
        })`,
        returnByValue: true,
    });
    return response.result.value;
}

const version = await fetch(`${endpoint}/json/version`).then((response) => response.json());
const pageTarget = (await targets()).find(
    (target) => target.type === "page" && target.url === base,
);
if (!pageTarget) throw new Error("Chrome 独立配置中没有打开看板");
const page = await connect(pageTarget.webSocketDebuggerUrl);
try {
    const status = await inspectWindow(page);
    const manifest = await page.call("Page.getAppManifest");
    const { installabilityErrors } = await page.call("Page.getInstallabilityErrors");
    const manifestId = manifest.manifest?.id ?? null;
    console.log(
        JSON.stringify({ product: version.Browser, manifestId, status, installabilityErrors }),
    );
    if (!status?.board || !status.worker || !manifestId || installabilityErrors.length) {
        process.exitCode = 1;
    }
} finally {
    page.close();
}
