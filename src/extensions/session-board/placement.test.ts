import assert from "node:assert/strict";
import { test } from "node:test";
import {
    type HostId,
    type HostSectionId,
    type LogicalSectionId,
    localHostId,
    type SidebarContext,
} from "./desktop/connection.js";
import { sectionBindings } from "./placement.js";

test("section preflight only accepts unique replicas on the queried host", () => {
    function section(
        name: string,
        local?: string,
        remote?: string,
    ): SidebarContext["sections"][number] {
        return {
            id: name as LogicalSectionId,
            name,
            hostSectionIds: {
                ...(local ? { local: local as HostSectionId } : {}),
                ...(remote ? { "remote-test": remote as HostSectionId } : {}),
            },
            itemKeys: [],
        };
    }
    const context = {
        accountId: "account",
        userId: "user",
        sections: [
            section("进行中", "local-duplicate", "remote-active"),
            section("待审阅", "local-duplicate", "remote-review"),
            section("已阻塞", "local-blocked", "remote-blocked"),
            section("已阻塞"),
            section("待归档", undefined, "remote-archive"),
            section("待判断", "local-unknown"),
        ],
    };
    assert.deepEqual(sectionBindings(context, localHostId, "four"), []);
    assert.deepEqual(sectionBindings(context, localHostId, "five"), [
        { state: "unknown", label: "待判断", sectionId: "local-unknown" },
    ]);
    assert.deepEqual(sectionBindings(context, "remote-test" as HostId, "four"), [
        { state: "in_progress", label: "进行中", sectionId: "remote-active" },
        { state: "review", label: "待审阅", sectionId: "remote-review" },
        { state: "to_archive", label: "待归档", sectionId: "remote-archive" },
    ]);
});
