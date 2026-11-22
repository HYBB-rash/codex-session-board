import assert from "node:assert/strict";
import test from "node:test";
import { trustedAppFrame } from "./ipc-guard.js";
import { parseUiPayload, SidebarRegistry } from "./registry.js";

const page = { title: "Session Board", body: "Codex Enhancer UI integration successful." };

test("sidebar registration rejects a duplicate id and dispose removes only that item", () => {
  const registry = new SidebarRegistry();
  const remove = registry.register("session-board", {
    id: "session-board",
    title: "Session Board",
    route: "/session-board",
    icon: "layout-dashboard",
    page,
  });
  assert.equal(registry.list().length, 1);
  assert.throws(
    () => registry.register("other", { id: "session-board", title: "Other", route: "/other", page }),
    /already registered: session-board/,
  );
  remove();
  assert.deepEqual(registry.list(), []);
});

test("sidebar registration rejects ids and routes that cannot be used as stable keys", () => {
  const registry = new SidebarRegistry();
  assert.throws(() => registry.register("ext", { id: "../x", title: "X", route: "/x", page }), /id is invalid/);
  assert.throws(
    () => registry.register("ext", { id: "session-board", title: "Session Board", route: "session-board", page }),
    /route is invalid/,
  );
});

test("ui payload keeps the registered page text", () => {
  const state = parseUiPayload({
    autoOpen: true,
    items: [{ id: "session-board", title: "Session Board", route: "/session-board", page }],
  });
  assert.equal(state.autoOpen, true);
  assert.equal(state.items[0]?.page.body, page.body);
});

test("ipc accepts only the top-level app frame", () => {
  assert.equal(trustedAppFrame({ url: "app://-/index.html", parent: null }), true);
  assert.equal(trustedAppFrame({ url: "https://chatgpt.com/", parent: null }), false);
  assert.equal(trustedAppFrame({ url: "app://-/index.html", parent: {} }), false);
  assert.equal(trustedAppFrame(null), false);
  assert.equal(trustedAppFrame({ url: "app://-/index.html" } as { url: string; parent: unknown }), false);
});
