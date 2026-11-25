import { defineComponent } from "vue";

export const ReplyRefresh = defineComponent({
    props: {
        action: { type: String, required: true },
    },
    emits: ["activate"],
    setup(props, { emit }) {
        return () => (
            <button
                id="reply-refresh"
                type="button"
                hidden={!props.action}
                onClick={() => emit("activate")}
            >
                {props.action}
            </button>
        );
    },
});

export const ReplyOpen = defineComponent({
    props: { canOpen: { type: Boolean, required: true } },
    emits: ["activate"],
    setup(props, { emit }) {
        return () => (
            <footer class="reply-actions">
                <button
                    id="reply-open"
                    class="primary"
                    type="button"
                    disabled={!props.canOpen}
                    onClick={() => emit("activate")}
                >
                    进入原会话 ↗
                </button>
            </footer>
        );
    },
});
