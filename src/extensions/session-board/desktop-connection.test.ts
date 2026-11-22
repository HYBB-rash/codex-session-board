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
