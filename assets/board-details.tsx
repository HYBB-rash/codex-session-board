import { defineComponent, onMounted, onUpdated, onUnmounted, ref } from "vue";
import type { PropType } from "vue";
import type { ProjectTag } from "./board-view";

export const CardProject = defineComponent({
  props: { project: { type: Object as PropType<ProjectTag>, required: true } },
  setup(props) {
    return () => <details class={["project-tag", { pending: props.project.pending }]}>
      <summary title={props.project.summary + "，点击查看项目详情"}
        aria-label={props.project.summary + "，点击查看项目详情"}>{props.project.summary}</summary>
      <div class="project-info">{props.project.info.join("\n")}</div>
    </details>;
  },
});

export const CardNote = defineComponent({
  props: { note: { type: String, required: true } },
  setup(props) {
    const details = ref<HTMLDetailsElement>();
    const measure = () => {
      const node = details.value;
      const preview = node?.querySelector<HTMLElement>(".note-preview");
      if (!node || !preview || node.open) return;
      const overflow = preview.scrollHeight > preview.clientHeight + 1;
      node.classList.toggle("expandable", overflow);
      node.querySelector("summary")!.tabIndex = overflow ? 0 : -1;
    };
    const observer = new ResizeObserver(measure);
    onMounted(() => { observer.observe(details.value!); measure(); });
    onUpdated(measure);
    onUnmounted(() => observer.disconnect());
    return () => <details ref={details} class="card-note note-details" onToggle={measure}>
      <summary data-action="note" onClick={event => {
        if (!details.value?.classList.contains("expandable")) event.preventDefault();
      }}>
        <span class="note-preview">{props.note}</span>
        <span class="note-toggle note-more">查看完整备注</span>
        <span class="note-toggle note-less">收起备注</span>
      </summary>
      <div class="note-full">{props.note}</div>
    </details>;
  },
});
