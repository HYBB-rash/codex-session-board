import { computed, reactive } from "vue";

export interface ArchiveTarget {
  id: string; title: string; version: number; inputVersion: number; sectionVersion: string | null;
}
interface Item extends ArchiveTarget {
  status: "awaiting" | "pending" | "archived" | "verified" | "newInput" | "rejected";
  message?: string | null;
}
interface Batch { id: string; createdAt: number; finishedAt: number | null; items: Item[] }
interface Submission {
  id: string; index: number; total: number; current: string | null; submitted: Set<string>; sending: boolean;
}
export interface ArchiveRequests {
  request(path: string, body?: object): Promise<unknown>;
  changed(): void;
  refresh(ids: Set<string>): void;
}

function batchId() {
  if (crypto.randomUUID) return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0,8)}-${hex.slice(8,12)}-${hex.slice(12,16)}-${hex.slice(16,20)}-${hex.slice(20)}`;
}

export function archiveBatches(actions: ArchiveRequests) {
  const state = reactive({
    batches: [] as Batch[], run: null as Submission | null, loaded: false, checking: false, readError: false, notice: "",
  });
  const canSubmit = computed(() => state.loaded && !state.checking && !state.run);
  const toCheck = computed(() => state.batches.filter(batch => batch.items.some(item => item.status === "pending")));
  let readVersion = 0;
  function status(item: Item, batch: Batch) {
    if (state.run?.id !== batch.id || item.status !== "awaiting") return item.status;
    if (state.run.current === item.id) return "submitting";
    return state.run.submitted.has(item.id) ? "pending" : "queued";
  }
  const pending = computed(() => new Set(state.batches.flatMap(batch => batch.items
    .filter(item => ["pending", "submitting"].includes(status(item, batch))).map(item => item.id))));
  const find = (id: string) => state.batches.find(batch => batch.id === id);
  function receive(batch: Batch) {
    ++readVersion;
    const previous = find(batch.id);
    if (previous) {
      const known = new Map(previous.items.map(item => [item.id, item]));
      batch = { ...batch, finishedAt: batch.finishedAt ?? previous.finishedAt, items: batch.items.map(item => {
        const earlier = known.get(item.id);
        if (earlier && (item.status === "awaiting" ||
          item.status === "pending" && earlier.status !== "awaiting" && earlier.status !== "pending")) return earlier;
        return item;
      }) };
    }
    state.batches = [batch, ...state.batches.filter(record => record.id !== batch.id)]
      .sort((first, second) => second.createdAt - first.createdAt);
    actions.changed();
  }
  async function request<T>(path: string, body?: object, timeout = 30000) {
    let timer: number | undefined;
    try {
      return await Promise.race([actions.request(path, body) as Promise<T>, new Promise<never>((_, reject) => {
        timer = window.setTimeout(() => reject(new Error("归档请求超时")), timeout);
      })]);
    } finally { window.clearTimeout(timer); }
  }
  async function read() {
    const version = ++readVersion;
    try {
      const {batches} = await request<{batches: Batch[]}>("/api/archive-batches", undefined, 8000);
      if (version !== readVersion) return;
      if (!Array.isArray(batches)) throw new Error("归档记录格式不正确");
      const changed = new Set<string>();
      for (const batch of batches) {
        const previous = new Map(find(batch.id)?.items.map(item => [item.id,item.status]));
        for (const item of batch.items) {
          if (item.status !== "awaiting" && previous.get(item.id) !== item.status) changed.add(item.id);
        }
      }
      state.batches = batches;
      state.loaded = true;
      state.readError = false;
      if (changed.size) actions.refresh(changed);
      else actions.changed();
    } catch {
      if (version !== readVersion) return;
      state.readError = true;
      actions.changed();
    }
  }
  async function check(batches: Batch[], manual = false) {
    if (state.checking || !batches.length) return;
    ++readVersion;
    state.checking = true;
    state.notice = "";
    actions.changed();
    try {
      for (const {id} of batches)
        receive((await request<{batch: Batch}>("/api/archive-batches/check", {id}, 8000)).batch);
      if (manual && pending.value.size) state.notice = "刚刚核对，仍有结果待确认。";
    } catch { state.notice = "暂时无法核对结果，请稍后再试。"; }
    finally {
      state.checking = false;
      actions.refresh(new Set(batches.flatMap(batch => batch.items.filter(item => item.status === "pending").map(item => item.id))));
    }
  }
  async function submit(items: ArchiveTarget[]) {
    if (!canSubmit.value || !items.length) return;
    const id = batchId();
    ++readVersion;
    state.run = {id, index:0, total:items.length, current:null, submitted:new Set(), sending:true};
    state.notice = "";
    let timer: number | undefined;
    try {
      let batch: Batch | undefined;
      try { batch = (await request<{batch: Batch}>("/api/archive-batches", {id,items})).batch; }
      catch { await read(); batch = find(id); }
      if (!batch) throw new Error("缺少归档批次记录");
      receive(batch);
      for (const [index, target] of items.entries()) {
        if (find(id)?.items.find(item => item.id === target.id)?.status !== "awaiting") continue;
        state.run.index = index + 1;
        state.run.current = target.id;
        state.run.submitted.add(target.id);
        try {
          const {id:targetId, version, inputVersion, sectionVersion} = target;
          const {item} = await request<{item: Item}>("/api/archive", {id:targetId, version, inputVersion, sectionVersion, batchId:id});
          const latest = find(id);
          if (latest) receive({...latest, items: latest.items.map(row => row.id === targetId ? item : row)});
        } catch { await read(); }
        state.run.current = null;
        if (timer === undefined && toCheck.value.some(record => record.id === id))
          timer = window.setTimeout(() => { void check(toCheck.value.filter(record => record.id === id)); }, 1200);
      }
      state.run.sending = false;
      const {batch: finished} = await request<{batch: Batch}>("/api/archive-batches/finish", {id});
      receive(finished);
      await check(toCheck.value.filter(record => record.id === id));
    } catch {
      state.notice = "未收到本批操作的确认。请从「归档结果」核对记录。";
      await read();
    } finally {
      window.clearTimeout(timer);
      actions.refresh(state.run.submitted);
      state.run = null;
      actions.changed();
    }
  }
  async function restore() {
    await read();
    if (!state.run) await check(toCheck.value);
  }
  return {state, canSubmit, status, toCheck, pending, read, check, restore, submit};
}
