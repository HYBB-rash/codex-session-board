import { defineComponent } from "vue";
import type { PropType } from "vue";
import type { BoardDependencies, BoardModel, BoardSession } from "./board-view";
import { CardNote, CardProject } from "./board-details";

export default defineComponent({
  props: {
    session: { type: Object as PropType<BoardSession>, required: true },
    model: { type: Object as PropType<BoardModel>, required: true },
    dependencies: { type: Object as PropType<BoardDependencies>, required: true },
  },
  setup(props) {
    return () => {
      const { session, model, dependencies } = props;
      const pending = model.moving?.id === session.id;
      const locked = model.saving || model.creating || pending;
      const readOnly = dependencies.readOnly(session);
      const archivePending = model.archivePending.includes(session.id);
      const adjustTitle = (session.state === null ? "归列：" : "调整状态或备注：") + session.title;
      return <article class={["card", { "move-pending": pending, offline: session.canOpen === false }]}
        data-id={session.id} aria-label={session.title} aria-busy={pending ? "true" : undefined}
        draggable={!locked && !readOnly} onDragstart={event => dependencies.cardDrag(session, event)}
        onDragend={event => dependencies.cardDrag(session, event)}
        onClick={event => {
          const control = (event.target as Element).closest("button,a,input,select,textarea,details");
          if (!pending && !control) dependencies.preview(session);
        }}>
        <button class="title card-title" type="button" data-action="preview" disabled={pending}
          title={"预览回复：" + session.title} aria-controls="reply-reader"
          aria-expanded={model.selectedId === session.id}
          onClick={event => dependencies.preview(session, event.detail === 0)}>{session.title}</button>
        <CardProject project={dependencies.project(session)} />
        {session.state === null && session.placementReason && <div class="placement-reason">{session.placementReason}</div>}
        {session.remote && session.archived && <div class="meta">已归档</div>}
        {session.remote && session.presence === "cachedUnknown" && <div class="follow">
          {session.catalogCurrent ? "上次目录快照，当前状态待确认" : "历史缓存，当前目录未确认此会话"}</div>}
        {pending ? <div class="move-label">等待保存确认 · 临时落位</div> :
          model.unconfirmed.includes(session.id) && <div class="move-label">上次保存未收到确认，请核对实际位置</div>}
        {session.note && <CardNote note={session.note} />}
        {!session.manual && session.state === "blocked" && <div class="card-note">{session.reason}</div>}
        {session.followUp && <div class="follow">有新输入，待复核</div>}
        {archivePending && <div class="follow">归档结果待核对</div>}
        {readOnly && <div class="follow">当前仅供查看</div>}
        {session.observationError && <div class="follow">历史读取失败，保留原状态</div>}
        {session.observationWarning && <div class="follow">{session.observationWarning}</div>}
        {session.projectIssue && <div class="follow">项目归属待确认</div>}
        {session.titleSyncPending && <div class="follow">{session.titleSyncError ?
          "状态已保存，标题同步待重试：" + session.titleSyncError : "状态已保存，标题正在同步"}</div>}
        <div class="card-footer">
          <time title={"会话更新时间：" + dependencies.date(session.updatedAt)}>{dependencies.date(session.updatedAt)}</time>
          <div class="card-actions">
            <button type="button" class="row-action" data-action="adjust" title={adjustTitle} aria-label={adjustTitle}
              disabled={locked || readOnly} onClick={() => dependencies.edit(session.id)}>{session.state === null ? "归列" : "调整"}</button>
            {dependencies.canAssign(session) && <button type="button" class="row-action" data-action="assign-project"
              disabled={locked} onClick={() => dependencies.assign(session)}>归入项目</button>}
            {dependencies.archiveEligible(session) && !archivePending && <button type="button" class="row-action"
              data-action="archive" disabled={locked}
              title={session.followUp ? "请先打开会话核对输入；确认仅保存待归档状态，不执行归档" : "直接归档此 Codex 会话"}
              onClick={event => dependencies.archive(session, event.currentTarget as HTMLButtonElement)}>
              {session.followUp ? "确认待归档" : "归档"}</button>}
          </div>
        </div>
      </article>;
    };
  },
});
