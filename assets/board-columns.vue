<script setup lang="ts">
import type { BoardDependencies, BoardModel } from "./board-view";
import SessionCard from "./board-card";

defineProps<{ model: BoardModel; dependencies: BoardDependencies; unplaced?: boolean }>();
</script>

<template>
  <template v-if="unplaced">
    <SessionCard v-for="session in model.unplaced" :key="session.id"
      :session="session" :model="model" :dependencies="dependencies" />
    <div v-if="model.moving?.from === null" class="move-origin">
      移往{{ model.states[model.moving.state] }} · 等待确认
    </div>
  </template>
  <template v-else>
    <section v-for="column in model.columns" :id="'column-' + column.state" :key="column.state"
    :class="['column', column.state]" :data-state="column.state" :aria-label="column.label"
    :data-mode="model.legacyMode ? 'legacy' : 'native'"
    @dragover="dependencies.columnDrag(column.state, $event)"
    @dragleave="dependencies.columnDrag(column.state, $event)"
    @drop="dependencies.columnDrag(column.state, $event)">
    <div class="column-heading">
      <h2>{{ column.label }}</h2>
      <span class="column-count" :aria-label="column.sessions.some(session => session.id === model.moving?.id)
        ? `${column.sessions.length - 1} 条已确认，1 条待确认` : `${column.sessions.length} 条会话`">
        {{ column.sessions.some(session => session.id === model.moving?.id)
          ? `${column.sessions.length - 1} + 1` : String(column.sessions.length) }}
      </span>
      <span class="sort-slot" style="display:contents"></span>
      <button v-if="column.state === 'to_archive' && !model.legacyMode" type="button"
        class="row-action" data-archive-all=""
        :disabled="!column.archiveCount" :title="model.scope + '；只归档本列可归档的会话'"
        @click="dependencies.archiveAll(column.sessions)">
        归档 {{ column.archiveCount }} 条
      </button>
    </div>
    <div class="cards">
      <SessionCard v-for="session in column.sessions" :key="session.id"
        :session="session" :model="model" :dependencies="dependencies" />
      <div v-if="model.moving?.from === column.state" class="move-origin">
        移往{{ model.states[model.moving.state] }} · 等待确认
      </div>
      <div v-if="!column.sessions.length && model.moving?.from !== column.state" class="empty-column">
        {{ column.unread ? "尚未读取会话，可查看连接详情" : column.state === "to_archive" && model.archivePending.length ?
          "暂无匹配的会话。归档结果待核对，见「归档结果」。" : "暂无匹配的会话" }}
      </div>
    </div>
    </section>
  </template>
</template>
