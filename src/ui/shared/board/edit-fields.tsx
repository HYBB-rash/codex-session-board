import { defineComponent, type PropType } from "vue";

export default defineComponent({
    props: {
        state: { type: String, required: true },
        note: { type: String, required: true },
        deferReason: { type: String, required: true },
        canDefer: Boolean,
        states: { type: Object as PropType<Record<string, string>>, required: true },
        unplaced: Boolean,
        busy: Boolean,
    },
    emits: {
        state: (_value: string) => true,
        note: (_value: string) => true,
        deferReason: (_value: string) => true,
    },
    setup(props, { emit }) {
        return () => (
            <>
                <label for="state">状态</label>
                <select
                    id="state"
                    value={props.state}
                    required
                    disabled={props.busy}
                    onChange={(event) => emit("state", (event.target as HTMLSelectElement).value)}>
                    {props.unplaced && (
                        <option value="" disabled>
                            选择状态
                        </option>
                    )}
                    {Object.entries(props.states).map(([value, label]) => (
                        <option
                            key={value}
                            value={value}
                            disabled={value === "deferred" && !props.canDefer}>
                            {label}
                        </option>
                    ))}
                </select>
                {props.state === "deferred" && (
                    <>
                        <label for="defer-reason">搁置原因（必填）</label>
                        <textarea
                            id="defer-reason"
                            value={props.deferReason}
                            maxlength={2000}
                            required
                            readonly={props.busy}
                            onInput={(event) =>
                                emit("deferReason", (event.target as HTMLTextAreaElement).value)
                            }
                        />
                    </>
                )}
                <label for="note">备注（可选）</label>
                <textarea
                    id="note"
                    value={props.note}
                    maxlength={2000}
                    readonly={props.busy}
                    placeholder="记录下一步"
                    onInput={(event) => emit("note", (event.target as HTMLTextAreaElement).value)}
                />
            </>
        );
    },
});
