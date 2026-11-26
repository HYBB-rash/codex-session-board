<template>
<button type="button" id="archive-history-open" class="quiet" @click="dialog?.showModal()">
    已归档会话
</button>
<dialog ref="dialog" class="archive-history" aria-labelledby="archive-history-title" @close="back">
    <header class="archive-heading">
        <h2 id="archive-history-title">{{ selected?.title ?? "已归档会话" }}</h2>
        <button type="button" class="quiet" @click="dialog?.close()">关闭</button>
    </header>
    <p v-if="message" class="feedback" role="status">{{ message }}</p>
    <template v-if="selected">
        <button type="button" class="quiet" @click="back">返回已归档会话</button>
        <p v-if="loading" role="status">正在读取会话…</p>
        <p v-else-if="turns && !turns.some((turn) => turn.messages.length)">此会话没有文本记录。</p>
        <div v-else class="archive-transcript">
            <div v-for="turn in turns" :key="turn.id">
                <section v-for="item in turn.messages" :key="item.id">
                    <strong>{{ item.role === "userMessage" ? "你" : "Codex" }}</strong>
                    <div>{{ item.text }}</div>
                </section>
            </div>
        </div>
    </template>
    <template v-else>
        <input v-model="query" type="search" placeholder="搜索标题、主机或项目" aria-label="搜索已归档会话">
        <p v-if="!matches.length">{{ query ? "没有匹配的会话。" : "暂无已归档会话。" }}</p>
        <ul v-else class="archive-entries">
            <li v-for="session in matches" :key="session.id" :data-archive-id="session.id">
                <div>
                    <strong>{{ session.title }}</strong>
                    <div class="meta">{{ session.hostLabel }} · {{ project(session) || session.cwd || "默认工作区" }}</div>
                    <div class="meta">
                        {{ session.archivedAt ? `归档回执 ${date(session.archivedAt)}` : "归档时间未知" }}
                        <span v-if="session.checkedAt"> · 核对于 {{ date(session.checkedAt) }}</span>
                        <span v-if="!session.canOpen"> · 上次确认，主机暂不可用</span>
                    </div>
                </div>
                <div class="archive-entry-actions">
                    <button type="button" class="row-action" :disabled="!session.canOpen" @click="open(session)">
                        打开
                    </button>
                    <button type="button" class="row-action" :disabled="!session.canRestore || restoring.has(session.id)" @click="restoreArchive(session.id)">
                        {{ restoring.has(session.id) ? "恢复中…" : "恢复" }}
                    </button>
                </div>
            </li>
        </ul>
    </template>
</dialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import type { BoardSession } from "../session";

interface HistoryTurn {
    id: string;
    messages: { id: string; role: string; text: string }[];
}

const props = defineProps<{
    sessions(): BoardSession[];
    date(timestamp?: number): string;
    project(session: BoardSession): string | undefined;
    read(id: string): Promise<unknown>;
    restore(id: string): Promise<string | undefined>;
}>();
const dialog = ref<HTMLDialogElement>();
const query = ref("");
const selected = ref<BoardSession>();
const turns = ref<HistoryTurn[]>();
const message = ref("");
const loading = ref(false);
const restoring = reactive(new Set<string>());
let readSequence = 0;

const matches = computed(() => {
    const text = query.value.trim().toLowerCase();
    return props.sessions()
        .filter((session) => [session.title, session.hostLabel, props.project(session), session.cwd]
            .filter(Boolean).join(" ").toLowerCase().includes(text))
        .sort((left, right) => (right.archivedAt ?? right.checkedAt ?? 0) - (left.archivedAt ?? left.checkedAt ?? 0));
});

function back() {
    readSequence++;
    selected.value = undefined;
    turns.value = undefined;
    message.value = "";
}

async function open(session: BoardSession) {
    const sequence = ++readSequence;
    selected.value = session;
    message.value = "";
    loading.value = true;
    try {
        const result = await props.read(session.id) as { turns: HistoryTurn[] };
        if (sequence === readSequence) turns.value = result.turns;
    } catch (error) {
        if (sequence === readSequence)
            message.value = error instanceof Error ? error.message : String(error);
    } finally {
        if (sequence === readSequence) loading.value = false;
    }
}

async function restoreArchive(id: string) {
    if (restoring.has(id)) return;
    restoring.add(id);
    try {
        message.value = (await props.restore(id)) ?? "会话已恢复";
    } finally {
        restoring.delete(id);
    }
}
</script>
