<template>
<dialog
    ref="dialog"
    id="editor"
    aria-labelledby="editor-heading"
    :closedby="busy ? 'none' : 'closerequest'"
    @keydown.esc="busy && $event.preventDefault()"
    @cancel.prevent="cancel"
    @close="closed">
    <form ref="form" id="edit-form" @submit.prevent="save">
        <h2 id="editor-heading">调整工作状态</h2>
        <div id="dialog-title" class="dialog-title">
            {{ session?.title }}{{ session?.remote ? ` · ${session.hostLabel}` : "" }}
        </div>
        <EditFields
            :states="states"
            :state="state"
            :unplaced="session?.state === null"
            :note="note"
            :defer-reason="deferReason"
            :can-defer="session?.canDefer !== false"
            :busy="busy"
            @state="state = $event"
            @note="changeNote"
            @defer-reason="changeDeferReason"/>
        <div id="reason" class="reason">{{ session?.placementReason || session?.reason }}</div>
        <div id="notice" class="notice" role="alert">{{ notice }}</div>
        <div class="dialog-actions">
            <button v-if="failed" type="button" :disabled="busy" @click="refresh">
                重新核对
            </button>
            <button id="cancel" type="button" :disabled="busy" @click="cancel">取消</button>
            <button
                id="save"
                type="submit"
                class="primary"
                :disabled="busy"
                @click.prevent="form?.reportValidity() && save()">
                保存状态
            </button>
        </div>
    </form>
</dialog>
</template>
<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, shallowRef } from "vue";
import type { BoardSession } from "../session";
import EditFields from "../shared/board/edit-fields";
import { readDraft, writeDraft } from "./drafts";
import type { EditorPorts } from "./mount";

const { ports } = defineProps<{ ports: EditorPorts }>();
const dialog = ref<HTMLDialogElement>();
const form = ref<HTMLFormElement>();
const session = shallowRef<BoardSession>();
const states = shallowRef<Record<string, string>>({});
const state = ref("");
const note = ref("");
const deferReason = ref("");
const notice = ref("");
const busy = ref(false);
const failed = ref(false);
let retained: string | null = null;
let noteEdited = false;
let deferReasonEdited = false;

async function open(card: BoardSession, options: Record<string, string>, target?: string) {
    if (session.value || !dialog.value) return;
    session.value = card;
    states.value = options;
    retained = readDraft(card);
    noteEdited = retained !== null;
    note.value = retained ?? card.note;
    deferReason.value = card.deferReason ?? "";
    deferReasonEdited = false;
    state.value = target || card.state || "";
    failed.value = false;
    notice.value = retained === null ? "" : "已恢复上次未保存的备注，请核对当前状态后保存。";
    dialog.value.showModal();
    await nextTick();
    const field = state.value === "deferred" ? "#defer-reason" : "#note";
    dialog.value?.querySelector<HTMLTextAreaElement>(field)?.focus();
}

function changeDeferReason(value: string) {
    deferReasonEdited = true;
    deferReason.value = value;
}

function changeNote(value: string) {
    noteEdited = true;
    note.value = value;
    if (session.value) writeDraft(session.value, value);
}

function cancel() {
    if (busy.value || !session.value) return;
    writeDraft(session.value, retained);
    dismiss();
}

function dismiss() {
    busy.value = false;
    dialog.value?.close();
    closed();
}

function closed() {
    if (dialog.value?.open) return;
    if (busy.value && session.value) {
        dialog.value?.showModal();
        return;
    }
    const card = session.value;
    session.value = undefined;
    if (card) ports.closed(card);
}

function reasonForSave() {
    if (state.value !== "deferred") return;
    const reason = deferReason.value.trim();
    if (!reason) {
        notice.value = "请填写搁置原因";
        dialog.value?.querySelector<HTMLTextAreaElement>("#defer-reason")?.focus();
    }
    return reason;
}

function retainFailedNote(card: BoardSession) {
    if (!noteEdited) return " 请重新核对当前状态和备注。";
    retained = note.value;
    return writeDraft(card, note.value)
        ? " 备注草稿已保留，请重新核对后保存。"
        : " 备注仍在编辑框中，请保留后重新核对。";
}

async function save() {
    const card = session.value;
    if (
        !card ||
        busy.value ||
        (!Object.hasOwn(states.value, state.value) && state.value !== "auto")
    )
        return;
    const reason = reasonForSave();
    if (reason === "") return;
    busy.value = true;
    notice.value = "";
    try {
        await ports.save(card, state.value === "auto" ? null : state.value, note.value, reason);
        if (session.value !== card) return;
        writeDraft(card, null);
        dismiss();
    } catch (error) {
        if (session.value !== card) return;
        failed.value = true;
        notice.value = (error as Error).message + retainFailedNote(card);
    } finally {
        busy.value = false;
    }
}

async function refresh() {
    const card = session.value;
    if (!card || busy.value) return;
    busy.value = true;
    try {
        const latest = await ports.refresh(card);
        if (session.value !== card) return;
        session.value = latest;
        state.value = latest.state || "";
        if (!noteEdited) note.value = latest.note;
        if (!deferReasonEdited) deferReason.value = latest.deferReason ?? "";
        failed.value = false;
        notice.value = noteEdited
            ? "已读取当前状态，备注草稿已保留。请核对后保存。"
            : "已读取当前状态和备注，请核对后保存。";
    } catch (error) {
        notice.value = (error as Error).message;
    } finally {
        busy.value = false;
    }
}

defineExpose({ open });
onBeforeUnmount(() => {
    session.value = undefined;
});
</script>
