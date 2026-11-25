import { computed, reactive } from "vue";

export interface ArchiveTarget {
    id: string;
    title: string;
    version: number;
    inputVersion: number;
    sectionVersion: string | null;
}
interface Item extends ArchiveTarget {
    status: "awaiting" | "pending" | "archived" | "verified" | "newInput" | "rejected";
    message?: string | null;
}
interface Batch {
    id: string;
    createdAt: number;
    finishedAt: number | null;
    items: Item[];
}
interface Submission {
    id: string;
    index: number;
    total: number;
    current: string | null;
    submitted: Set<string>;
    sending: boolean;
}
export interface ArchiveRequests {
    request(path: string, body?: object): Promise<unknown>;
    changed(): void;
    refresh(ids: Set<string>): void;
}

function batchId() {
    if (crypto.randomUUID) return crypto.randomUUID();
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    const versionByte = bytes[6] ?? 0;
    const variantByte = bytes[8] ?? 0;
    bytes[6] = (versionByte & 15) | 64;
    bytes[8] = (variantByte & 63) | 128;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function archiveBatches(actions: ArchiveRequests) {
    return new ArchiveBatches(actions);
}

class ArchiveBatches {
    readonly state = reactive({
        batches: [] as Batch[],
        run: null as Submission | null,
        loaded: false,
        checking: false,
        readError: false,
        notice: "",
    });
    readonly canSubmit = computed(
        () => this.state.loaded && !this.state.checking && !this.state.run,
    );
    readonly toCheck = computed(() =>
        this.state.batches.filter((batch) => batch.items.some((item) => item.status === "pending")),
    );
    private readVersion = 0;
    constructor(private readonly actions: ArchiveRequests) {}

    readonly status = (item: Item, batch: Batch) => {
        const { run } = this.state;
        if (run?.id !== batch.id || item.status !== "awaiting") return item.status;
        if (run.current === item.id) return "submitting";
        return run.submitted.has(item.id) ? "pending" : "queued";
    };

    readonly pending = computed(
        () =>
            new Set(
                this.state.batches.flatMap((batch) =>
                    batch.items
                        .filter((item) =>
                            ["pending", "submitting"].includes(this.status(item, batch)),
                        )
                        .map((item) => item.id),
                ),
            ),
    );
    private find(id: string) {
        return this.state.batches.find((batch) => batch.id === id);
    }

    private receive(batch: Batch) {
        ++this.readVersion;
        const { state } = this;
        const previous = this.find(batch.id);
        if (previous) {
            const known = new Map(previous.items.map((item) => [item.id, item]));
            batch = {
                ...batch,
                finishedAt: batch.finishedAt ?? previous.finishedAt,
                items: batch.items.map((item) => {
                    const earlier = known.get(item.id);
                    if (
                        earlier &&
                        (item.status === "awaiting" ||
                            (item.status === "pending" &&
                                earlier.status !== "awaiting" &&
                                earlier.status !== "pending"))
                    )
                        return earlier;
                    return item;
                }),
            };
        }
        state.batches = [batch, ...state.batches.filter((record) => record.id !== batch.id)].sort(
            (first, second) => second.createdAt - first.createdAt,
        );
        this.actions.changed();
    }

    private async request<T>(path: string, body?: object, timeout = 30000) {
        let timer: number | undefined;
        try {
            return await Promise.race([
                this.actions.request(path, body) as Promise<T>,
                new Promise<never>((_, reject) => {
                    timer = window.setTimeout(() => reject(new Error("归档请求超时")), timeout);
                }),
            ]);
        } finally {
            window.clearTimeout(timer);
        }
    }
    readonly read = async () => {
        const { state, actions } = this;
        const version = ++this.readVersion;
        try {
            const { batches } = await this.request<{ batches: Batch[] }>(
                "/api/archive-batches",
                undefined,
                8000,
            );
            if (version !== this.readVersion) return;
            if (!Array.isArray(batches)) throw new Error("归档记录格式不正确");
            const changed = new Set<string>();
            for (const batch of batches) {
                const previous = new Map(
                    this.find(batch.id)?.items.map((item) => [item.id, item.status]),
                );
                for (const item of batch.items) {
                    if (item.status !== "awaiting" && previous.get(item.id) !== item.status)
                        changed.add(item.id);
                }
            }
            state.batches = batches;
            state.loaded = true;
            state.readError = false;
            if (changed.size) actions.refresh(changed);
            else actions.changed();
        } catch {
            if (version !== this.readVersion) return;
            state.readError = true;
            actions.changed();
        }
    };

    readonly check = async (batches: Batch[], manual = false) => {
        const { state, actions } = this;
        if (state.checking || !batches.length) return;
        ++this.readVersion;
        state.checking = true;
        state.notice = "";
        actions.changed();
        try {
            for (const { id } of batches)
                this.receive(
                    (
                        await this.request<{ batch: Batch }>(
                            "/api/archive-batches/check",
                            { id },
                            8000,
                        )
                    ).batch,
                );
            if (manual && this.pending.value.size) state.notice = "刚刚核对，仍有结果待确认。";
        } catch {
            state.notice = "暂时无法核对结果，请稍后再试。";
        } finally {
            state.checking = false;
            actions.refresh(
                new Set(
                    batches.flatMap((batch) =>
                        batch.items
                            .filter((item) => item.status === "pending")
                            .map((item) => item.id),
                    ),
                ),
            );
        }
    };

    private async registerBatch(id: string, items: ArchiveTarget[]) {
        let batch: Batch | undefined;
        try {
            batch = (await this.request<{ batch: Batch }>("/api/archive-batches", { id, items }))
                .batch;
        } catch {
            await this.read();
            batch = this.find(id);
        }
        if (!batch) throw new Error("缺少归档批次记录");
        this.receive(batch);
    }

    private async submitTarget(run: Submission, target: ArchiveTarget) {
        const { id: targetId, version, inputVersion, sectionVersion } = target;
        try {
            const { item } = await this.request<{ item: Item }>("/api/archive", {
                id: targetId,
                version,
                inputVersion,
                sectionVersion,
                batchId: run.id,
            });
            const latest = this.find(run.id);
            if (latest)
                this.receive({
                    ...latest,
                    items: latest.items.map((row) => (row.id === targetId ? item : row)),
                });
        } catch {
            await this.read();
        }
    }

    private pendingBatches(id: string) {
        return this.toCheck.value.filter((record) => record.id === id);
    }

    private async submitTargets(
        run: Submission,
        items: ArchiveTarget[],
        scheduleCheck: () => void,
    ) {
        for (const [index, target] of items.entries()) {
            if (
                this.find(run.id)?.items.find((item) => item.id === target.id)?.status !==
                "awaiting"
            )
                continue;
            run.index = index + 1;
            run.current = target.id;
            run.submitted.add(target.id);
            await this.submitTarget(run, target);
            run.current = null;
            scheduleCheck();
        }
    }

    readonly submit = async (items: ArchiveTarget[]) => {
        const { state, actions } = this;
        if (!this.canSubmit.value || !items.length) return;
        const id = batchId();
        ++this.readVersion;
        state.run = {
            id,
            index: 0,
            total: items.length,
            current: null,
            submitted: new Set(),
            sending: true,
        };
        const run = state.run;
        state.notice = "";
        let timer: number | undefined;
        try {
            await this.registerBatch(id, items);
            await this.submitTargets(run, items, () => {
                if (timer === undefined && this.pendingBatches(id).length)
                    timer = window.setTimeout(() => {
                        void this.check(this.pendingBatches(id));
                    }, 1200);
            });
            run.sending = false;
            const { batch: finished } = await this.request<{ batch: Batch }>(
                "/api/archive-batches/finish",
                { id },
            );
            this.receive(finished);
            await this.check(this.pendingBatches(id));
        } catch {
            state.notice = "未收到本批操作的确认。请从「归档结果」核对记录。";
            await this.read();
        } finally {
            window.clearTimeout(timer);
            actions.refresh(run.submitted);
            state.run = null;
            actions.changed();
        }
    };

    readonly restore = async () => {
        await this.read();
        if (!this.state.run) await this.check(this.toCheck.value);
    };
}
