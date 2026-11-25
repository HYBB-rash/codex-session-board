import { defineComponent, type PropType } from "vue";

export default defineComponent({
    props: {
        state: { type: String, required: true },
        note: { type: String, required: true },
        states: { type: Object as PropType<Record<string, string>>, required: true },
        unplaced: Boolean,
        busy: Boolean,
    },
    emits: { state: (_value: string) => true, note: (_value: string) => true },
    setup(props, { emit }) {
        return () => (
            <>
                <label for="state">状态</label>
                <select
                    id="state"
                    value={props.state}
                    required
                    disabled={props.busy}
                    onChange={(event) => emit("state", (event.target as HTMLSelectElement).value)}
                >
                    {props.unplaced && (
                        <option value="" disabled>
                            选择状态
                        </option>
                    )}
                    {Object.entries(props.states).map(([value, label]) => (
                        <option key={value} value={value}>
                            {label}
                        </option>
                    ))}
                </select>
                <label for="note">备注 / 阻塞原因（可选）</label>
                <textarea
                    id="note"
                    value={props.note}
                    maxlength={2000}
                    readonly={props.busy}
                    aria-describedby="note-help"
                    placeholder="记录下一步，或说明卡在哪里"
                    onInput={(event) => emit("note", (event.target as HTMLTextAreaElement).value)}
                />
                <p id="note-help" class="reason">
                    {props.state === "blocked"
                        ? "请说明卡在哪里，方便之后继续处理；暂时不填也可以保存。"
                        : "可以记录下一步，也可以留空。"}
                </p>
            </>
        );
    },
});
