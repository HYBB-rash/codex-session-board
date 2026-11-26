<template>
<details v-if="model?.hostTaskState" :key="model.accountScope" class="tasks">
    <summary>全部任务</summary>
    <p v-for="host in model.health.unsupportedRemoteHosts" :key="host.hostId" role="status">
        {{ host.hostLabel }} · {{ host.reason }}
    </p>
    <details v-for="group in groups" :key="group.state" class="task-group">
        <summary>{{ group.label }} <span>{{ group.rows.length }}</span></summary>
        <div v-for="card in group.rows" :key="card.id" class="task-row">
            <button
                class="task-title"
                type="button"
                :disabled="!card.canOpen"
                :title="card.title"
                @click="open(card.id)">
                {{ card.title }}
            </button>
            <small v-if="card.remote || card.presence === 'cachedUnknown'">
                {{ card.hostLabel }}{{ cachedLabel(card) }}
            </small>
            <button
                v-if="!card.readOnly"
                type="button"
                :data-task-edit="card.id"
                :aria-label="`调整 ${card.title}`"
                @click="edit(card, $event)">
                调整
            </button>
        </div>
    </details>
    <p v-if="message" role="alert">{{ message }}</p>
</details>
<div ref="editorHost"></div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef } from "vue";
import type { BoardService } from "../../../extensions/service";
import { mountCardEditor } from "../../edit/mount";
import type { BoardSession } from "../../session";
import { sortColumn } from "../../sorting";

const { board } = defineProps<{ board: BoardService }>();
const model = shallowRef<ReturnType<BoardService["snapshot"]>>();
const message = ref("");
const editorHost = ref<HTMLElement>();
let editor: ReturnType<typeof mountCardEditor> | undefined;
let trigger: HTMLElement | undefined;
let stop: (() => void) | undefined;

const groups = computed(() => {
    const snapshot = model.value;
    if (!snapshot) return [];
    const sessions = snapshot.sessions.filter((session) => !session.archived);
    const named = Object.entries(snapshot.states).map(([state, label]) => ({
        state,
        label,
        rows: sortColumn(state, sessions.filter((session) => session.state === state), snapshot.columnSorts).sessions,
    }));
    const unplaced = sessions.filter((session) => session.state === null);
    return unplaced.length
        ? [...named, { state: "", label: "未归列会话", rows: sortColumn(null, unplaced).sessions }]
        : named;
});

async function open(id: string) {
    message.value = "";
    try {
        await board.request("/api/open", { id });
    } catch (error) {
        message.value = error instanceof Error ? error.message : String(error);
    }
}

function edit(card: BoardSession, event: MouseEvent) {
    const snapshot = model.value;
    if (!snapshot) return;
    trigger = event.currentTarget as HTMLElement;
    editor?.open(card, snapshot.states);
}

function cachedLabel(card: BoardSession) {
    if (card.presence !== "cachedUnknown") return "";
    return card.taskStateConfirmedAt
        ? ` · 离线，确认于 ${new Date(card.taskStateConfirmedAt * 1000).toLocaleString()}`
        : " · 离线，尚未确认";
}

function restoreFocus() {
    if (!trigger) return;
    if (trigger.isConnected) {
        trigger.focus();
        return;
    }
    const root = editorHost.value?.getRootNode() as ShadowRoot | undefined;
    root?.querySelector<HTMLElement>(".tasks > summary")?.focus();
}

onMounted(() => {
    if (!editorHost.value) return;
    editor = mountCardEditor(editorHost.value, {
        async save(card, state, note, deferReason) {
            const { id, version, inputVersion, sectionVersion } = card;
            await board.request("/api/state", { id, state, note, deferReason, version, inputVersion, sectionVersion });
        },
        async refresh(card) {
            await board.refresh();
            const next = board.snapshot().sessions.find(({ id, accountScope }) =>
                id === card.id && accountScope === card.accountScope,
            );
            if (!next || next.readOnly) throw new Error("这条会话暂不可修改，请检查主机连接");
            return next;
        },
        closed: restoreFocus,
    });
    stop = board.subscribe((snapshot) => {
        if (model.value?.accountScope && model.value.accountScope !== snapshot.accountScope) {
            editorHost.value?.querySelector("dialog")?.close();
            message.value = "";
        }
        model.value = snapshot;
    });
});
onUnmounted(() => {
    stop?.();
    editor?.unmount();
});
</script>
