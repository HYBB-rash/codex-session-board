import assert from "node:assert/strict";
import { test } from "node:test";
import { DesktopConnection, hostId, localHostId, type ThreadId } from "./desktop-connection.js";

const remoteName = "remote-ssh-discovered:fixture";
const thread = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee" as ThreadId;

test("identical thread IDs on different hosts require matching native response hosts", async () => {
    const original = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    const frames: Array<{ hostId: string; request: { id: string } }> = [];
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId, remoteName] }),
        async send(frame) {
            frames.push(frame as (typeof frames)[number]);
        },
        async sidebar() {},
    });
    try {
        desktop.knownHosts();
        const remote = hostId(remoteName);
        assert.ok(remote);
        const localRead = desktop.read(desktop.capture(localHostId), thread, false);
        const remoteRead = desktop.read(desktop.capture(remote), thread, false);
        assert.equal(frames.length, 2);
        const [localFrame, remoteFrame] = frames;
        assert.ok(localFrame && remoteFrame);
        assert.notEqual(localFrame.request.id, remoteFrame.request.id);
        desktop.receive({
            type: "mcp-response",
            hostId: remote,
            message: { id: localFrame.request.id },
        });
        await assert.rejects(localRead, /回执主机不一致/);
        desktop.receive({
            type: "mcp-response",
            hostId: remote,
            message: { id: remoteFrame.request.id, result: { thread: { id: thread } } },
        });
        assert.equal((await remoteRead).id, thread);
    } finally {
        Reflect.set(globalThis, "window", original);
    }
});

test("a disconnected catalog host retires pending reads and prior connection epochs", async () => {
    const original = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    let catalog = [localHostId, remoteName];
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: catalog }),
        async send() {},
        async sidebar() {},
    });
    try {
        const remote = hostId(remoteName);
        assert.ok(remote);
        desktop.knownHosts();
        const before = desktop.capture(remote);
        const reading = desktop.read(before, thread, false);
        catalog = [localHostId];
        desktop.knownHosts();
        await assert.rejects(reading, /连接已变更/);
        assert.throws(() => desktop.capture(remote), /连接已断开/);
        catalog = [localHostId, remoteName];
        desktop.knownHosts();
        assert.throws(() => desktop.assertCurrent(before), /连接已变更/);
        assert.equal(desktop.capture(remote).hostId, remote);
    } finally {
        Reflect.set(globalThis, "window", original);
    }
});

test("a bounded native call ignores a reply received after its deadline", async () => {
    const original = Reflect.get(globalThis, "window");
    Reflect.set(globalThis, "window", globalThis);
    let lateReply: string | undefined;
    const desktop = new DesktopConnection({
        bootstrap: () => ({ catalogHostIds: [localHostId] }),
        async send(frame) {
            lateReply = (frame as { request: { id: string } }).request.id;
        },
        async sidebar() {},
    });
    try {
        desktop.knownHosts();
        const connection = desktop.capture(localHostId);
        await assert.rejects(desktop.read(connection, thread, false, 10), /请求超时/);
        desktop.receive({
            type: "mcp-response",
            hostId: localHostId,
            message: { id: lateReply, error: { message: "late" } },
        });
        desktop.assertCurrent(connection);
    } finally {
        Reflect.set(globalThis, "window", original);
    }
});
