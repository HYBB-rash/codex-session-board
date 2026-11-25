import type { Thread, Turn } from "./desktop/connection.js";
import { type BoardState, labels } from "./placement.js";

export interface Phase {
    key: string;
    turnId: string;
    state: BoardState;
    at: number | null;
    reason: string;
    terminal: boolean;
}

interface RuntimeDecision {
    state: Extract<BoardState, "in_progress" | "blocked" | "review">;
    reason: string;
}

export function turnInput(turn: Turn | undefined): string | null {
    if (!turn?.id || turn.itemsView === "notLoaded") return null;
    const users = turn.items
        .filter(({ type }) => type === "userMessage")
        .map(({ id }) => id)
        .sort();
    return users.length ? JSON.stringify([turn.id, users]) : null;
}

export function finalAnswer(turn: Turn | undefined): string | null {
    if (turn?.status !== "completed") return null;
    return (
        turn.items.findLast(
            (item) =>
                item.type === "agentMessage" && item.phase === "final_answer" && item.text?.trim(),
        )?.text ?? null
    );
}

function validTime(value: number | null | undefined): value is number {
    return typeof value === "number" && Number.isFinite(value) && value > 0;
}

export function runtimeState(
    status: Thread["status"],
    turn: Pick<Turn, "status" | "completedAt">,
): RuntimeDecision | null {
    if (status.type === "active") {
        if (turn.status !== "inProgress") return null;
        const flags = status.activeFlags ?? [];
        if (flags.includes("waitingOnApproval"))
            return { state: "blocked", reason: "等待你批准后继续执行。" };
        if (flags.includes("waitingOnUserInput"))
            return { state: "blocked", reason: "执行暂停，等待你回答交互问题。" };
        return { state: "in_progress", reason: "AI 正在执行本轮任务。" };
    }
    if (status.type === "systemError")
        return { state: "blocked", reason: "执行遇到系统异常，需要处理后继续。" };
    const observedTerminal =
        status.type === "idle" ||
        (status.type === "notLoaded" &&
            (turn.status === "completed" || validTime(turn.completedAt)));
    if (!observedTerminal) return null;
    if (turn.status === "completed")
        return { state: "review", reason: "本轮执行已结束，等待你审阅结果。" };
    if (turn.status === "failed")
        return { state: "blocked", reason: "本轮请求失败，需要处理后重试。" };
    if (turn.status === "interrupted")
        return { state: "blocked", reason: "本轮执行已中断，需要检查后继续。" };
    return null;
}

export function runtimePhase(status: Thread["status"], turn: Turn | undefined): Phase | null {
    if (!turn?.id) return null;
    const terminal = status.type === "idle" || status.type === "notLoaded";
    if (terminal && !validTime(turn.completedAt)) return null;
    const decision = runtimeState(status, turn);
    if (!decision) return null;
    const flags = status.type === "active" ? [...new Set(status.activeFlags)].sort() : [];
    const time =
        terminal || status.type === "systemError"
            ? (turn.completedAt ?? turn.startedAt)
            : turn.startedAt;
    return {
        ...decision,
        turnId: turn.id,
        key: JSON.stringify([turn.id, decision.state, flags]),
        at: validTime(time) ? time : null,
        terminal,
    };
}

export function phaseIsFromTurn(key: string | undefined, turnId: string): boolean {
    return key?.startsWith(`${JSON.stringify([turnId]).slice(0, -1)},`) ?? false;
}

const knownPrefixes = Object.values(labels).flatMap((label) => [`[${label}]`, `【${label}】`]);

export function projectedTitle(title: string, state: BoardState | null): string {
    if (state !== null && !Object.hasOwn(labels, state)) throw new Error("未知状态");
    let body = title;
    for (
        let prefix = knownPrefixes.find((item) => body.startsWith(item));
        prefix;
        prefix = knownPrefixes.find((item) => body.startsWith(item))
    ) {
        body = body.slice(prefix.length).trimStart();
    }
    const content = body || "未命名会话";
    return state === null ? content : `[${labels[state]}] ${content}`;
}
