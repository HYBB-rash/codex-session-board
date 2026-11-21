import { computed, createApp, nextTick, ref, shallowRef, Teleport } from "vue";
import { archiveBatches, type ArchiveRequests } from "./board-archive-batches";

const labels = {
  awaiting: "尚无提交记录", queued: "等候提交", submitting: "正在提交", pending: "待核对",
  archived: "已归档", verified: "核实时已归档", newInput: "未归档 · 有新输入", rejected: "未归档",
};
interface Board { legacyMode?: boolean; sessions: { id: string; canOpen?: boolean }[] }
interface Actions extends ArchiveRequests {
  open(id: string): void;
  date(timestamp: number): string;
}

function mountArchiveResults(host: HTMLElement, actions: Actions) {
  const {state, canSubmit, status, toCheck, pending, read, check, restore, submit} = archiveBatches(actions);
  const board = shallowRef<Board>({ sessions: [] });
  const expanded = ref(false);
  const toggle = ref<HTMLButtonElement>();
  const current = computed(() => state.batches.find(batch => batch.id === state.run?.id) ?? state.batches[0]);
  const canOpen = (item: {id: string}) => board.value.sessions.find(session => session.id === item.id)?.canOpen !== false;
  const openButton = (item: {id: string}) => <button type="button" class="row-action"
    onClick={() => actions.open(item.id)}>打开会话</button>;
  async function toggleDetails() {
    expanded.value = !expanded.value;
    if (expanded.value && !state.run && !state.checking) await read();
  }
  async function manualCheck(event: MouseEvent) {
    const button = event.currentTarget;
    await check(toCheck.value, true);
    await nextTick();
    if (button instanceof HTMLElement && !button.isConnected && document.activeElement === document.body)
      toggle.value?.focus({ preventScroll: true });
  }
  createApp({ setup: () => () => {
    const batch = current.value, run = state.run, legacy = board.value.legacyMode === true;
    const statuses = batch?.items.map(item => status(item, batch)) ?? [];
    const count = (value: string) => statuses.filter(entry => entry === value).length;
    const counts = batch ? `已确认归档 ${count("archived") + count("verified")} / ${statuses.length} 条` +
      Object.entries({submitting:"条正在提交",pending:"条待核对",queued:"条等候提交",awaiting:"条尚无提交记录",
        newInput:"条有新输入",rejected:"条未归档"}).filter(([key]) => count(key))
        .map(([key, label]) => ` · ${count(key)} ${label}`).join("") : "";
    const progress = run?.sending ? run.index ? `正在提交第 ${run.index} / ${run.total} 条` : "正在记录本次归档" : "";
    const newInputs = batch?.items.filter(item => item.status === "newInput") ?? [];
    const openable = newInputs.filter(canOpen);
    return <>
      <Teleport to="#archive-results-control"><button ref={toggle} id="archive-results-toggle" class="quiet"
        type="button" hidden={legacy} aria-controls="archive-result-list" aria-expanded={expanded.value}
        onClick={toggleDetails}>归档结果</button></Teleport>
      <section class="archive-results" aria-label="手动归档结果" hidden={legacy}>
        <div id="archive-summary" class="archive-summary" hidden={!progress && !counts && !state.notice && !state.readError}>
          <div id="archive-summary-text" role="status"><strong>{progress || counts}</strong>
            {progress && counts && <p>{counts}</p>}
            {progress ? <p>请保持看板打开，直到本批提交结束。</p> : batch?.finishedAt ?
              <p>本批提交已结束。可以关闭看板，之后从「归档结果」查看。</p> :
              !!toCheck.value.length && <p>下次打开看板时会再尝试核对。结果仍可在「归档结果」查看。</p>}
            {!progress && !!count("awaiting") && <p>尚无提交记录的会话仍可从待归档卡片另行提交。</p>}
            {!!newInputs.length && <p>核对新输入后，回到卡片点「确认待归档」。确认后需另行发起归档。</p>}
            {state.notice && <p>{state.notice}</p>}
            {state.readError && <p>暂时无法读取归档记录。<button type="button" class="row-action" onClick={read}>重新读取</button></p>}
          </div>
          <div id="archive-summary-actions">
            {!!toCheck.value.length && !progress && <><button type="button" class="row-action"
              disabled={state.checking || !!run} onClick={manualCheck}>{state.checking ? "正在核对…" : "核对结果"}</button> 仅查询归档状态</>}
            {openable.length === 1 && openButton(openable[0])}
          </div>
        </div>
        <div id="archive-result-list" hidden={!expanded.value}>
          {expanded.value && state.batches.map(record => <div key={record.id} class="archive-batch">
            <strong>{actions.date(record.createdAt)} 提交 · {record.items.length} 条</strong>
            <ul class="archive-items">{record.items.map(item => <li key={item.id}><div>{item.title}
              {item.status === "newInput" && canOpen(item) && openButton(item)}{item.message && <p>{item.message}</p>}
            </div><span class="archive-item-status">{labels[status(item, record)]}</span></li>)}</ul>
          </div>)}
          {expanded.value && !state.batches.length && <p>{state.readError ? "暂时无法读取归档记录。" :
            state.loaded ? "暂无归档记录。" : "正在读取归档记录…"}</p>}
        </div>
      </section>
    </>;
  }}).mount(host);
  return { restore, submit, get canSubmit() { return canSubmit.value; }, get pending() { return pending.value; },
    updateBoard(value: Board) { board.value = value; } };
}

declare global { interface Window { mountArchiveResults: typeof mountArchiveResults } }
window.mountArchiveResults = mountArchiveResults;
