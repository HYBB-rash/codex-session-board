import type { BoardSession } from "./board-view";

export type Direction = "asc" | "desc";
export interface SortPreference {
  field: "updated_at";
  direction: Direction;
  version: number;
}
export interface SortMenuProps {
  state: string;
  label: string;
  preference: SortPreference;
  disabled: boolean;
  canChange: () => boolean;
  save: (state: string, preference: SortPreference) => Promise<void>;
}

function validTime(value: number) {
  return Number.isFinite(value) && value > 0 && value <= 8640000000000;
}

export function sortColumn(state: string | null, sessions: BoardSession[], saved?: Record<string, SortPreference>) {
  const preference: SortPreference = (state && saved?.[state]) || {
    field: "updated_at", direction: state === "to_archive" ? "asc" : "desc", version: 0,
  };
  if (preference.field !== "updated_at") throw new Error("不支持的排序字段，请刷新看板。");
  sessions.sort((first, second) => {
    const left = validTime(first.updatedAt), right = validTime(second.updatedAt);
    if (left !== right) return left ? -1 : 1;
    if (left && first.updatedAt !== second.updatedAt)
      return preference.direction === "asc" ? first.updatedAt - second.updatedAt : second.updatedAt - first.updatedAt;
    return first.id < second.id ? -1 : first.id > second.id ? 1 : 0;
  });
  return { sessions, preference };
}
