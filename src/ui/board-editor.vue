<script lang="ts">
const transientNotes = new Map<string, string>();
</script>
<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, shallowRef } from "vue";
import type { EditorPorts } from "./board-editor";
import type { BoardSession } from "./board-view";
import EditFields from "./board-edit-fields";

const { ports } = defineProps<{ ports: EditorPorts }>();
const dialog = ref<HTMLDialogElement>();
const form = ref<HTMLFormElement>();
const session = shallowRef<BoardSession>();
const states = shallowRef<Record<string, string>>({});
const state = ref("");
const note = ref("");
const notice = ref("");
const busy = ref(false);
const failed = ref(false);
let retained: string | null = null;
let noteEdited = false;

function draftKey(card: BoardSession) {
  const key = [card.hostId || "local", card.threadId || card.id];
  return "card-note:" + JSON.stringify(card.accountScope ? [card.accountScope, ...key] : key);
}

function writeDraft(card: BoardSession, value: string | null) {
  const key = draftKey(card);
  if (value === null) transientNotes.delete(key);
  else transientNotes.set(key, value);
  try {
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, value);
    if (!card.accountScope && (!card.remote || card.id.startsWith("remote:")))
      sessionStorage.removeItem("failed-note:" + card.id);
    return true;
  } catch { return false; }
}

function readDraft(card: BoardSession) {
  const key = draftKey(card);
  if (transientNotes.has(key)) return transientNotes.get(key)!;
  try {
    const stored = sessionStorage.getItem(key);
    if (stored !== null || card.accountScope || card.remote && !card.id.startsWith("remote:")) return stored;
    const legacy = sessionStorage.getItem("failed-note:" + card.id);
    if (legacy !== null) writeDraft(card, legacy);
    return legacy;
  } catch { return null; }
}

function open(card: BoardSession, options: Record<string, string>, target?: string) {
  if (session.value) return;
  session.value = card;
  states.value = options;
  retained = readDraft(card);
  noteEdited = retained !== null;
  note.value = retained ?? card.note;
  state.value = target || card.state || "";
  failed.value = false;
  notice.value = retained === null ? "" : "已恢复上次未保存的备注，请核对当前状态后保存。";
  dialog.value!.showModal();
  nextTick(() => dialog.value?.querySelector("textarea")?.focus());
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
  dialog.value!.close();
  closed();
}

function closed() {
  if (dialog.value?.open) return;
  if (busy.value && session.value) { dialog.value?.showModal();return; }
  const card = session.value;
  session.value = undefined;
  if (card) ports.closed(card);
}

async function save() {
  const card = session.value;
  if (!card || busy.value || !Object.hasOwn(states.value, state.value) && state.value !== "auto") return;
  busy.value = true;
  notice.value = "";
  try {
    await ports.save(card, state.value === "auto" ? null : state.value, note.value);
    if (session.value !== card) return;
    writeDraft(card, null);
    dismiss();
  } catch (error) {
    if (session.value !== card) return;
    failed.value = true;
    notice.value = (error as Error).message;
    if (noteEdited) {
      retained = note.value;
      notice.value += writeDraft(card, note.value) ? " 备注草稿已保留，请重新核对后保存。" : " 备注仍在编辑框中，请保留后重新核对。";
    } else notice.value += " 请重新核对当前状态和备注。";
  } finally { busy.value = false; }
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
    failed.value = false;
    notice.value = noteEdited ? "已读取当前状态，备注草稿已保留。请核对后保存。" : "已读取当前状态和备注，请核对后保存。";
  } catch (error) { notice.value = (error as Error).message; }
  finally { busy.value = false; }
}

defineExpose({ open });
onBeforeUnmount(() => { session.value = undefined; });
</script>
<template>
  <dialog ref="dialog" id="editor" aria-labelledby="editor-heading" :closedby="busy ? 'none' : 'closerequest'"
    @keydown.esc="busy && $event.preventDefault()" @cancel.prevent="cancel" @close="closed">
    <form ref="form" id="edit-form" @submit.prevent="save">
      <h2 id="editor-heading">调整工作状态</h2>
      <div id="dialog-title" class="dialog-title">{{ session?.title }}{{ session?.remote ? " · " + session.hostLabel : "" }}</div>
      <EditFields :states="states" :state="state" :unplaced="session?.state === null" :note="note" :busy="busy"
        @state="state = $event" @note="changeNote" />
      <div id="reason" class="reason">{{ session?.placementReason || session?.reason }}</div>
      <div id="notice" class="notice" role="alert">{{ notice }}</div>
      <div class="dialog-actions">
        <button v-if="failed" type="button" :disabled="busy" @click="refresh">重新核对</button>
        <button id="cancel" type="button" :disabled="busy" @click="cancel">取消</button>
        <button id="save" type="submit" class="primary" :disabled="busy" @click.prevent="form?.reportValidity() && save()">保存状态</button>
      </div>
    </form>
  </dialog>
</template>
