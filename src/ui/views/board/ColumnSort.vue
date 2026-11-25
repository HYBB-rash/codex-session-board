<template>
<button
    ref="trigger"
    type="button"
    class="column-sort"
    aria-haspopup="menu"
    :aria-expanded="expanded"
    :aria-controls="`sort-${state}`"
    :aria-label="`${label}排序，按更新时间${labels[preference.direction]}`"
    title="按会话更新时间排列；点击选择顺序，拖入本列后也遵循此顺序"
    :value="preference.direction"
    :disabled="disabled || submitting"
    @click="expanded ? close() : open()"
    @keydown.up.prevent="open"
    @keydown.down.prevent="open">
    <span class="sort-label">更新 {{preference.direction === "asc" ? "↑" : "↓"}}</span><span class="sort-caret" aria-hidden="true">▾</span>
</button>
<div
    ref="menu"
    :id="`sort-${state}`"
    popover="auto"
    role="menu"
    class="sort-menu"
    :aria-label="`${label}排序`"
    @beforetoggle="toggle"
    @keydown="navigate"
    @focusout="focusOut">
    <div class="sort-menu-title">按更新时间</div>
    <button
        v-for="direction in directions"
        :key="direction"
        ref="options"
        type="button"
        class="sort-option"
        tabindex="-1"
        role="menuitemradio"
        :data-direction="direction"
        :aria-checked="direction === preference.direction"
        @click="choose(direction)">
        {{labels[direction]}}
    </button>
</div>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onUpdated, ref, useTemplateRef } from "vue";
import type { Direction, SortMenuProps, SortPreference } from "./sorting";

const props = defineProps<SortMenuProps>();
const trigger = useTemplateRef<HTMLButtonElement>("trigger");
const menu = useTemplateRef<HTMLDivElement>("menu");
const options = useTemplateRef<HTMLButtonElement[]>("options");
const directions: Direction[] = ["desc", "asc"];
const labels = { desc: "新的在前", asc: "旧的在前" };
const expanded = ref(false);
const submitting = ref(false);
let openedPreference: SortPreference | undefined;
const active = () =>
    (trigger.value?.getRootNode() as Document | ShadowRoot | undefined)?.activeElement;

function place() {
    if (!trigger.value || !menu.value) return;
    const bounds = trigger.value.getBoundingClientRect();
    menu.value.style.left = `${Math.max(8, Math.min(bounds.right - 174, innerWidth - 182))}px`;
    menu.value.style.top = `${Math.max(8, Math.min(bounds.bottom + 6, innerHeight - 148))}px`;
}

function stopPositioning() {
    window.removeEventListener("resize", place);
    window.removeEventListener("scroll", place, true);
}
onBeforeUnmount(stopPositioning);
onUpdated(() => {
    if (expanded.value) place();
});

function toggle(event: ToggleEvent) {
    expanded.value = event.newState === "open";
    if (expanded.value) {
        window.addEventListener("resize", place);
        window.addEventListener("scroll", place, true);
    } else {
        stopPositioning();
    }
}

function focusOption(direction: Direction) {
    options.value
        ?.find((option) => option.getAttribute("data-direction") === direction)
        ?.focus({ preventScroll: true });
}

function open() {
    if (expanded.value || props.disabled || !props.canChange() || !menu.value) return;
    openedPreference = { ...props.preference };
    place();
    menu.value.showPopover();
    focusOption(openedPreference.direction);
}

function close() {
    menu.value?.hidePopover();
    trigger.value?.focus({ preventScroll: true });
}

function focusOut() {
    queueMicrotask(() => {
        if (expanded.value && !menu.value?.contains(active() ?? null) && active() !== trigger.value)
            menu.value?.hidePopover();
    });
}

function navigate(event: KeyboardEvent) {
    if (event.key === "Escape" || event.key === "Tab") {
        if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
        }
        close();
    } else if (["Home", "End", "ArrowDown", "ArrowUp"].includes(event.key)) {
        event.preventDefault();
        const current = active()?.getAttribute("data-direction");
        const index = current === "asc" || current === "desc" ? directions.indexOf(current) : -1;
        const next =
            event.key === "Home" ? 0 : event.key === "End" ? 1 : (index + 1) % directions.length;
        const direction = directions[next];
        if (direction) focusOption(direction);
    }
}

async function choose(direction: Direction) {
    close();
    if (
        !openedPreference ||
        submitting.value ||
        !props.canChange() ||
        direction === props.preference.direction
    )
        return;
    submitting.value = true;
    try {
        await props.save(props.state, { ...openedPreference, direction });
    } finally {
        submitting.value = false;
        await nextTick();
        if (document.activeElement === document.body) trigger.value?.focus({ preventScroll: true });
    }
}
</script>
