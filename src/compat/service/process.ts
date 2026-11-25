import { spawnSync } from "node:child_process";
import { readFileSync, readlinkSync } from "node:fs";

export function processStartTime(pid: number): number | null {
    const ps = process.platform === "darwin" ? "/bin/ps" : "/usr/bin/ps";
    const result = spawnSync(ps, ["-o", "lstart=", "-p", String(pid)], {
        encoding: "utf8",
        env: { ...process.env, LC_ALL: "C", TZ: "UTC0" },
        timeout: 1000,
    });
    if (result.status !== 0 || !result.stdout.trim()) return null;
    const time = Date.parse(`${result.stdout.trim()} UTC`);
    return Number.isFinite(time) ? time : null;
}

export function processStartIdentity(pid: number): string | null {
    if (process.platform === "darwin") {
        const time = processStartTime(pid);
        return time === null ? null : `darwin:${time}`;
    }
    if (process.platform !== "linux") return null;
    try {
        const boot = readFileSync("/proc/sys/kernel/random/boot_id", "utf8").trim();
        const namespace = readlinkSync(`/proc/${pid}/ns/pid`);
        const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
        const startTicks = stat.slice(stat.lastIndexOf(")") + 2).split(" ")[19];
        if (!startTicks || !/^\d+$/.test(startTicks)) return null;
        return `linux:${boot}:${namespace}:${startTicks}`;
    } catch {
        return null;
    }
}
