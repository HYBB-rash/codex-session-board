<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, shallowRef } from "vue";
import type { PreviewDependencies, PreviewSession } from "./reply-preview";
import { ReplyOpen, ReplyRefresh } from "./shared/reply-actions";

const props = defineProps<{ dependencies: PreviewDependencies; root: ShadowRoot | Document }>();
interface Reading {
    session: PreviewSession;
    observedTurnId: string | undefined;
    turnId?: string | null;
    html: string;
    busy: boolean;
    ready: boolean;
    status: string;
    action: string;
    error: boolean;
}
const reading = shallowRef<Reading>();
const reader = shallowRef<HTMLElement>();
const body = shallowRef<HTMLElement>();
const top = shallowRef("0px");
const selectedId = computed(() => reading.value?.session.id);
const metadata = computed(() => {
    const session = reading.value?.session;
    if (!session) return "";
    const date = session.updatedAt
        ? new Date(session.updatedAt * 1000).toLocaleString("zh-CN", {
              month: "numeric",
              day: "numeric",
              hour: "2-digit",
              minute: "2-digit",
          })
        : "尚未读取";
    return `${session.hostLabel || "本机"} · ${date}`;
});

function align() {
    const workspace = props.root.getElementById("board-workspace");
    if (workspace) top.value = `${workspace.getBoundingClientRect().top + 4}px`;
}
function update(sessions: PreviewSession[], failed: boolean) {
    const current = reading.value;
    if (!current) return;
    current.session =
        sessions.find((session) => session.id === current.session.id) || current.session;
    const latest = current.session.previewTurnId;
    if (
        current.ready &&
        latest &&
        latest !== current.turnId &&
        latest !== current.observedTurnId &&
        !failed
    ) {
        current.status = current.turnId ? "新答复已完成 · 当前仍在阅读上一条" : "新答复已完成";
        current.action = "查看新答复";
    }
}
function validReply(
    value: unknown,
): value is { turnId: string | null; html: string | null; inProgress: boolean } {
    if (!value || typeof value !== "object") return false;
    const { inProgress, turnId, html } = value as {
        inProgress?: unknown;
        turnId?: unknown;
        html?: unknown;
    };
    return (
        typeof inProgress === "boolean" &&
        ((turnId === null && html === null) ||
            (typeof turnId === "string" && typeof html === "string"))
    );
}
async function open(session: PreviewSession, focus = false) {
    if (!props.dependencies.canRead(session.id)) return;
    const current = reactive<Reading>({
        session,
        observedTurnId: session.previewTurnId,
        html: "",
        busy: true,
        ready: false,
        status: "正在读取回复…",
        action: "",
        error: false,
    });
    reading.value = current;
    align();
    await nextTick();
    if (reading.value !== current) return;
    if (body.value) body.value.scrollTop = 0;
    if (focus)
        reader.value?.querySelector<HTMLElement>("#reply-close")?.focus({ preventScroll: true });
    try {
        const result = await props.dependencies.read(session.id);
        if (reading.value !== current) return;
        if (!validReply(result)) throw new Error("读取结果无效");
        current.turnId = result.turnId;
        current.html = result.html || "";
        current.ready = true;
        current.status = !result.turnId
            ? "还没有完整回复"
            : result.inProgress
              ? "上一条完整回复 · 当前轮次进行中"
              : "最近一条完整回复";
        update([current.session], false);
    } catch {
        if (reading.value !== current) return;
        current.status = "读取回复失败。请重试或进入原会话核对。";
        current.action = "重试读取";
        current.error = true;
    } finally {
        if (reading.value === current) current.busy = false;
    }
}
function close(restoreFocus = false) {
    const current = reading.value;
    if (!current) return;
    const restore =
        restoreFocus ||
        reader.value?.contains(props.root.activeElement) ||
        document.activeElement === document.body;
    reading.value = undefined;
    if (!restore) return;
    const title = [...props.root.querySelectorAll<HTMLButtonElement>(".card-title")].find(
        (node) => node.closest(".card")?.getAttribute("data-id") === current.session.id,
    );
    const target =
        title && !title.disabled && title.checkVisibility()
            ? title
            : props.root.getElementById("host-filter");
    target?.focus({ preventScroll: true });
}
function refresh() {
    if (reading.value) void open(reading.value.session, true);
}
async function enterSession() {
    const current = reading.value;
    if (
        current &&
        (await props.dependencies.openSession(current.session.id)) &&
        reading.value === current
    )
        close();
}
let pointerStartedInside = false;
function pointerDown(event: PointerEvent) {
    pointerStartedInside = Boolean(
        event.target instanceof Node && reader.value?.contains(event.target),
    );
}
function outsideClick(event: MouseEvent) {
    if (
        !reading.value ||
        !props.dependencies.canDismiss() ||
        (event.detail > 0 && pointerStartedInside)
    )
        return;
    if (
        event.target instanceof Element &&
        !event.target.closest(
            "#reply-reader,.card,button,a,input,select,textarea,label,summary,dialog,[popover]",
        )
    )
        close();
}
function dismissOnEscape(event: KeyboardEvent) {
    if (
        event.key !== "Escape" ||
        !reading.value ||
        props.root.querySelector("dialog[open],.sort-menu:popover-open")
    )
        return;
    event.preventDefault();
    close(true);
}
function openLink(event: MouseEvent) {
    const link =
        event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
    if (!link) return;
    event.preventDefault();
    props.dependencies.openLink(link.href);
}
onMounted(() => {
    window.addEventListener("resize", align);
    props.root.addEventListener("pointerdown", pointerDown as EventListener, true);
    props.root.addEventListener("click", outsideClick as EventListener);
    props.root.addEventListener("keydown", dismissOnEscape as EventListener);
    if (body.value) {
        body.value.tabIndex = 0;
        body.value.addEventListener("click", openLink);
    }
});
onBeforeUnmount(() => {
    window.removeEventListener("resize", align);
    props.root.removeEventListener("pointerdown", pointerDown as EventListener, true);
    props.root.removeEventListener("click", outsideClick as EventListener);
    props.root.removeEventListener("keydown", dismissOnEscape as EventListener);
    body.value?.removeEventListener("click", openLink);
});
defineExpose({ selectedId, open, update });
</script>

<template>
    <div id="reply-scrim" aria-hidden="true" :hidden="!reading"></div>
    <aside
        id="reply-reader"
        ref="reader"
        aria-labelledby="reply-title"
        :hidden="!reading"
        :style="{ '--reply-board-top': top }"
    >
        <header class="reply-heading">
            <div class="reply-label">
                <span>回复预览</span
                ><button id="reply-close" class="quiet" type="button" @click="close()">关闭</button>
            </div>
            <h2 id="reply-title" :title="reading?.session.title">{{ reading?.session.title }}</h2>
            <p id="reply-meta" :title="metadata">{{ metadata }}</p>
        </header>
        <div id="reply-state">
            <p id="reply-status" role="status" :class="{ error: reading?.error }">
                {{ reading?.status }}
            </p>
            <ReplyRefresh :action="reading?.action || ''" @activate="refresh" />
        </div>
        <section
            id="reply-body"
            ref="body"
            aria-label="助手回复正文"
            :aria-busy="Boolean(reading?.busy)"
        >
            <div id="reply-text" v-html="reading?.html || ''"></div>
        </section>
        <ReplyOpen :can-open="reading?.session.canOpen !== false" @activate="enterSession" />
    </aside>
</template>
