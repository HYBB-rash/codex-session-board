<script setup lang="ts">
import { nextTick, ref, shallowRef } from "vue";
import type { EditorPorts, EditorSession } from "./board-editor";
import EditFields from "./board-edit-fields";

const { ports } = defineProps<{ ports: EditorPorts }>();
const dialog = ref<HTMLDialogElement>();
const session = shallowRef<EditorSession>();
const states = shallowRef<Record<string, string>>({});
const state = ref("");
const note = ref("");
const notice = ref("");
const busy = ref(false);
const failed = ref(false);
let retained: string | null = null;

function draftKey(card: EditorSession) {
  return "card-note:" + JSON.stringify([card.hostId || "local", card.threadId || card.id]);
}

function writeDraft(card: EditorSession, value: string | null) {
  try {
    if (value === null) sessionStorage.removeItem(draftKey(card));
    else sessionStorage.setItem(draftKey(card), value);
    if (!card.remote || card.id.startsWith("remote:")) sessionStorage.removeItem("failed-note:" + card.id);
    return true;
  } catch { return false; }
}

function readDraft(card: EditorSession) {
  try {
    const stored = sessionStorage.getItem(draftKey(card));
    if (stored !== null || card.remote && !card.id.startsWith("remote:")) return stored;
    const legacy = sessionStorage.getItem("failed-note:" + card.id);
    if (legacy !== null) writeDraft(card, legacy);
    return legacy;
  } catch { return null; }
}

function open(card: EditorSession, options: Record<string, string>, target?: string) {
  if (session.value) return;
  session.value = card;
  states.value = options;
  retained = readDraft(card);
  note.value = retained ?? card.note;
  state.value = target || card.state || "";
  failed.value = false;
  notice.value = retained === null ? "" : "已恢复上次未保存的备注，请核对当前状态后保存。";
  dialog.value!.showModal();
  nextTick(() => dialog.value?.querySelector("textarea")?.focus());
}

function changeNote(value: string) {
  note.value = value;
  if (session.value) writeDraft(session.value, value);
}

function cancel() {
  if (busy.value || !session.value) return;
  writeDraft(session.value, retained);
  dialog.value!.close();
}

function closed() {
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
    writeDraft(card, null);
    dialog.value!.close();
  } catch (error) {
    failed.value = true;
    retained = note.value;
    const stored = writeDraft(card, note.value);
    notice.value = String((error as Error).message) + (stored ? " 备注草稿已保留，请重新核对后保存。" : " 备注仍在编辑框中，请保留后重新核对。");
  } finally { busy.value = false; }
}

async function refresh() {
  const card = session.value;
  if (!card || busy.value) return;
  busy.value = true;
  try {
    const latest = await ports.refresh(card);
    session.value = latest;
    state.value = latest.state || "";
    failed.value = false;
    notice.value = "已读取当前状态，备注草稿已保留。请核对后保存。";
  } catch (error) { notice.value = (error as Error).message; }
  finally { busy.value = false; }
}

defineExpose({ open });
</script>
<template>
  <dialog ref="dialog" id="editor" aria-labelledby="editor-heading" @cancel.prevent="cancel" @close="closed">
    <form id="edit-form" @submit.prevent="save">
      <h2 id="editor-heading">调整工作状态</h2>
      <div id="dialog-title" class="dialog-title">{{ session?.title }}{{ session?.remote ? " · " + session.hostLabel : "" }}</div>
      <EditFields :states="states" :state="state" :unplaced="session?.state === null" :note="note" :busy="busy"
        @state="state = $event" @note="changeNote" />
      <div id="reason" class="reason">{{ session?.placementReason || session?.reason }}</div>
      <div id="notice" class="notice" role="alert">{{ notice }}</div>
      <div class="dialog-actions">
        <button v-if="failed" type="button" :disabled="busy" @click="refresh">重新核对</button>
        <button id="cancel" type="button" :disabled="busy" @click="cancel">取消</button>
        <button id="save" type="submit" class="primary" :disabled="busy">保存状态</button>
      </div>
    </form>
  </dialog>
</template>
