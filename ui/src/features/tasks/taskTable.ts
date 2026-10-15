import type { TaskSnapshot } from "../../bindings/TaskSnapshot";

export type TaskTable = Record<string, TaskSnapshot>;

const TERMINAL = new Set<TaskSnapshot["state"]>(["succeeded", "failed", "cancelled"]);

export function isTerminal(snapshot: TaskSnapshot): boolean {
  return TERMINAL.has(snapshot.state);
}

/** 手元より新しい revision のときだけ反映する。変化がなければ同じオブジェクトを返す。 */
export function mergeSnapshot(table: TaskTable, snapshot: TaskSnapshot): TaskTable {
  const current = table[snapshot.id];
  if (current && current.revision >= snapshot.revision) return table;
  return { ...table, [snapshot.id]: snapshot };
}

/** 一覧を正として置き換える。ただし手元の方が新しいタスクは手元を残す。 */
export function mergeList(table: TaskTable, list: TaskSnapshot[], preserveIds: ReadonlySet<string> = new Set()): TaskTable {
  const next: TaskTable = {};
  for (const id of preserveIds) {
    if (table[id]) next[id] = table[id];
  }
  for (const snapshot of list) {
    const current = table[snapshot.id];
    next[snapshot.id] = current && current.revision > snapshot.revision ? current : snapshot;
  }
  return next;
}

export function removeTask(table: TaskTable, id: string): TaskTable {
  if (!(id in table)) return table;
  const { [id]: _removed, ...rest } = table;
  return rest;
}

export function activeCount(table: TaskTable): number {
  return Object.values(table).filter((t) => !isTerminal(t)).length;
}

export function sortedTasks(table: TaskTable): TaskSnapshot[] {
  return Object.values(table).sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
