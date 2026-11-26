import { defineComponent, onBeforeUnmount, onMounted, type PropType, shallowRef } from "vue";
import type { BoardControls, BoardModel } from "./columns";
import type { BoardSession } from "./session";
import { CardNote, CardProject } from "./shared/card/details";

type CardActions = ReturnType<BoardControls["actions"]>;

function CardNotices({ session, actions }: { session: BoardSession; actions: CardActions }) {
    const titleSync = session.titleSyncError
        ? `状态已保存，标题同步待重试：${session.titleSyncError}`
        : "状态已保存，标题正在同步";
    return (
        <>
            {session.followUp && <div class="follow">有新输入，待复核</div>}
            {actions.pendingArchive && <div class="follow">归档结果待核对</div>}
            {actions.readOnly && <div class="follow">当前仅供查看</div>}
            {session.observationError && <div class="follow">历史读取失败，保留原状态</div>}
            {session.observationWarning && <div class="follow">{session.observationWarning}</div>}
            {session.projectIssue && <div class="follow">项目归属待确认</div>}
            {session.titleSyncPending && <div class="follow">{titleSync}</div>}
        </>
    );
}

function CardStatus({
    session,
    model,
    actions,
    pending,
}: {
    session: BoardSession;
    model: BoardModel;
    actions: CardActions;
    pending: boolean;
}) {
    const cached = session.catalogCurrent
        ? "上次目录快照，当前状态待确认"
        : "历史缓存，当前目录未确认此会话";
    return (
        <>
            {session.state === null && session.placementReason && (
                <div class="placement-reason">{session.placementReason}</div>
            )}
            {session.remote && session.archived && <div class="meta">已归档</div>}
            {session.remote && session.presence === "cachedUnknown" && (
                <div class="follow">{cached}</div>
            )}
            {pending && <div class="move-label">等待保存确认 · 临时落位</div>}
            {!pending && model.unconfirmed.includes(session.id) && (
                <div class="move-label">上次保存未收到确认，请核对实际位置</div>
            )}
            {session.note && <CardNote note={session.note} />}
            {session.state === "deferred" &&
                (session.deferReason ? (
                    <CardNote note={`搁置原因：${session.deferReason}`} />
                ) : (
                    <div class="card-note">待补搁置原因</div>
                ))}
            {!session.manual && session.state === "blocked" && (
                <div class="card-note">{session.reason}</div>
            )}
            <CardNotices session={session} actions={actions} />
        </>
    );
}

function CardFooter({
    session,
    dependencies,
    actions,
    locked,
}: {
    session: BoardSession;
    dependencies: BoardControls;
    actions: CardActions;
    locked: boolean;
}) {
    const adjustTitle = `${session.state === null ? "归列：" : "调整状态或备注："}${session.title}`;
    const updatedAt = dependencies.date(session.updatedAt);
    return (
        <div class="card-footer">
            <time title={`会话更新时间：${updatedAt}`}>{updatedAt}</time>
            <div class="card-actions">
                <button
                    type="button"
                    class="row-action"
                    data-action="adjust"
                    title={adjustTitle}
                    aria-label={adjustTitle}
                    disabled={!actions.canAdjust}
                    onClick={() => dependencies.edit(session.id)}>
                    {session.state === null ? "归列" : "调整"}
                </button>
                {dependencies.canAssign(session) && (
                    <button
                        type="button"
                        class="row-action"
                        data-action="assign-project"
                        disabled={locked}
                        onClick={() => dependencies.assign(session)}>
                        归入项目
                    </button>
                )}
                {actions.archive && (
                    <button
                        type="button"
                        class="row-action"
                        data-action="archive"
                        disabled={!actions.canSubmit}
                        title={
                            actions.archive === "confirm"
                                ? "请先打开会话核对输入；确认仅保存待归档状态，不执行归档"
                                : "直接归档此 Codex 会话"
                        }
                        onClick={() => dependencies.archive(session)}>
                        {actions.archive === "confirm" ? "确认待归档" : "归档"}
                    </button>
                )}
            </div>
        </div>
    );
}

export default defineComponent({
    props: {
        session: { type: Object as PropType<BoardSession>, required: true },
        model: { type: Object as PropType<BoardModel>, required: true },
        dependencies: { type: Object as PropType<BoardControls>, required: true },
    },
    setup(props) {
        const article = shallowRef<HTMLElement>();
        function setArticle(node: unknown) {
            article.value = node instanceof HTMLElement ? node : undefined;
        }
        function openFromBackground(event: MouseEvent) {
            const { session, model, dependencies } = props;
            if (model.moving?.id === session.id || !(event.target instanceof Element)) return;
            const control = event.target.closest("button,a,input,select,textarea,details");
            if (!control || !article.value?.contains(control)) dependencies.openCard(session);
        }
        onMounted(() => article.value?.addEventListener("click", openFromBackground));
        onBeforeUnmount(() => article.value?.removeEventListener("click", openFromBackground));
        return () => {
            const { session, model, dependencies } = props;
            const pending = model.moving?.id === session.id;
            const locked = model.saving || model.creating || pending;
            const actions = dependencies.actions(session);
            const preview = !dependencies.nativeConversation;
            return (
                <article
                    ref={setArticle}
                    class={[
                        "card",
                        {
                            "move-pending": pending,
                            offline: session.canOpen === false,
                            "drag-source": dependencies.drag.source?.id === session.id,
                        },
                    ]}
                    data-id={session.id}
                    aria-label={session.title}
                    aria-busy={pending ? "true" : undefined}
                    draggable={actions.canAdjust}
                    onDragstart={(event) => dependencies.drag.start(session, event)}
                    onDragend={dependencies.drag.cancel}>
                    <button
                        class="title card-title"
                        type="button"
                        data-action={preview ? "preview" : "open-conversation"}
                        disabled={pending}
                        title={`${preview ? "预览回复" : "打开会话"}：${session.title}`}
                        aria-controls={preview ? "reply-reader" : undefined}
                        aria-expanded={preview ? model.selectedId === session.id : undefined}
                        onClick={(event) => dependencies.openCard(session, event.detail === 0)}>
                        {session.title}
                    </button>
                    <CardProject project={dependencies.project(session)} />
                    <CardStatus
                        session={session}
                        model={model}
                        actions={actions}
                        pending={pending}
                    />
                    <CardFooter
                        session={session}
                        dependencies={dependencies}
                        actions={actions}
                        locked={locked}
                    />
                </article>
            );
        };
    },
});
