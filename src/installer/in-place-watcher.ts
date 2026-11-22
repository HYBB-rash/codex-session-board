import { readFileSync, rmSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import { atomicWrite } from "../extensions/session-board/records.js";

const systemUserUnits = "/etc/systemd/user";
const serviceName = "codex-enhancer-upgrade-check.service";
const pathName = "codex-enhancer-upgrade-check.path";
const serviceTargets = ["default.target", "graphical-session.target"];
const pathTargets = ["default.target"];

function unitPath(value: string): string {
    if (
        !isAbsolute(value) ||
        value !== value.trim() ||
        [...value].some((character) => character.charCodeAt(0) < 32)
    ) {
        throw new Error("systemd watcher paths must be absolute single-line paths");
    }
    return value.replaceAll("%", "%%");
}

export function systemWatcherFiles(commands: {
    node: string;
    cli: string;
    watch: string;
}): ReadonlyMap<string, string> {
    const node = JSON.stringify(unitPath(commands.node));
    const cli = JSON.stringify(unitPath(commands.cli));
    const directory = unitPath(commands.watch);
    if (directory.endsWith("\\"))
        throw new Error("the watched directory cannot end with a systemd line continuation");
    const service = [
        "[Unit]",
        "Description=Check the Codex Enhancer in-place installation",
        "StartLimitIntervalSec=0",
        "",
        "[Service]",
        "Type=oneshot",
        `ExecStart=:${node} ${cli} in-place-check`,
        "Restart=on-failure",
        "RestartSec=15s",
        "",
        "[Install]",
        `WantedBy=${serviceTargets.join(" ")}`,
        "",
    ].join("\n");
    const watcher = [
        "[Unit]",
        "Description=Watch the Codex Desktop installation directory",
        "",
        "[Path]",
        `PathChanged=${directory}`,
        `Unit=${serviceName}`,
        "",
        "[Install]",
        `WantedBy=${pathTargets.join(" ")}`,
        "",
    ].join("\n");
    return new Map([
        [join(systemUserUnits, serviceName), service],
        [join(systemUserUnits, pathName), watcher],
    ]);
}

export function systemWatcherLinks(): ReadonlyMap<string, string> {
    const links = new Map<string, string>();
    for (const target of serviceTargets)
        links.set(join(systemUserUnits, `${target}.wants`, serviceName), `../${serviceName}`);
    for (const target of pathTargets)
        links.set(join(systemUserUnits, `${target}.wants`, pathName), `../${pathName}`);
    return links;
}

export interface InPlaceUpgradeNotice {
    kind: "loader-replaced" | "incompatible" | "unverifiable";
    fingerprint: string;
    message: string;
}

export type InPlaceWatchStatus =
    | { kind: "ready" }
    | { kind: "not-installed" }
    | InPlaceUpgradeNotice;

function lastNotice(file: string): Pick<InPlaceUpgradeNotice, "kind" | "fingerprint"> | undefined {
    try {
        const stored = JSON.parse(
            readFileSync(file, "utf8"),
        ) as Partial<InPlaceUpgradeNotice> | null;
        if (typeof stored?.kind === "string" && typeof stored.fingerprint === "string")
            return { kind: stored.kind, fingerprint: stored.fingerprint };
        return undefined;
    } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT" || error instanceof SyntaxError)
            return undefined;
        throw error;
    }
}

export async function checkInPlaceUpgrade(
    privateDirectory: string,
    inspect: () => InPlaceWatchStatus | Promise<InPlaceWatchStatus>,
    notify: (notice: InPlaceUpgradeNotice) => void | Promise<void>,
): Promise<{ status: InPlaceWatchStatus; notified: boolean }> {
    const status = await inspect();
    const receipt = join(privateDirectory, "upgrade-watch.json");
    if (status.kind === "ready" || status.kind === "not-installed") {
        rmSync(receipt, { force: true });
        return { status, notified: false };
    }
    const previous = lastNotice(receipt);
    const { kind, fingerprint } = status;
    if (previous?.kind === kind && previous.fingerprint === fingerprint)
        return { status, notified: false };
    await notify(status);
    atomicWrite(receipt, { kind, fingerprint });
    return { status, notified: true };
}
